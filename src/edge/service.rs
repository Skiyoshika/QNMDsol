use std::collections::VecDeque;
use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::{channel, RecvTimeoutError, Sender};
use std::sync::{Arc, Mutex};
use std::thread::{self, JoinHandle};
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

use serde::Serialize;
use tiny_http::{Header, Request, Response, Server, StatusCode};

use crate::edge::config::EdgeConfig;
use crate::edge::recorder::{EdgeRecorder, RecordingMetadata};
use crate::openbci::OpenBciSession;
use crate::ssvep::{SsvepConfig, SsvepDecision, SsvepDecoder};

pub const CHANNEL_LABELS: [&str; 16] = [
    "Fp1", "Fp2", "C3", "C4", "P7", "P8", "O1", "O2", "F7", "F8", "F3", "F4", "T3", "T4", "P3",
    "P4",
];

const POSTERIOR_INDICES: [usize; 4] = [6, 7, 14, 15]; // O1, O2, P3, P4
const SIMULATED_SAMPLE_RATE_HZ: f32 = 250.0;
const SIMULATED_TARGET_HZ: f32 = 12.0;
const HISTORY_SECONDS: f32 = 4.0;

#[derive(Debug, Default, Clone, Serialize)]
pub struct EdgeStatus {
    pub connected: bool,
    pub streaming: bool,
    pub simulating: bool,
    pub sample_rate_hz: Option<f32>,
    pub eeg_channels: usize,
    pub channel_labels: Vec<String>,
    pub last_error: Option<String>,
}

pub struct EdgeState {
    pub status: EdgeStatus,
    pub channel_buffers: Vec<VecDeque<f32>>,
    pub buffer_capacity: usize,
    pub latest_decision: Option<SsvepDecision>,
}

impl Default for EdgeState {
    fn default() -> Self {
        Self {
            status: EdgeStatus::default(),
            channel_buffers: Vec::new(),
            buffer_capacity: 0,
            latest_decision: None,
        }
    }
}

/// Coarse verdict on whether the incoming EEG is trustworthy. Consumers
/// (VR / robot middleware) should ignore `latest_decision` unless this is
/// `Ok` — a railed headset will otherwise produce confident-looking but
/// meaningless frequency picks.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum SignalQuality {
    /// Signal is in a plausible µV range — decision can be used.
    Ok,
    /// Most channels are pinned at the ADS1299 full-scale rail
    /// (±187500 µV). Electrodes are not contacting scalp, or the
    /// reference (BIAS/SRB) is not connected. Decision is meaningless.
    Railed,
    /// No samples buffered yet (not connected / not streaming).
    NoSignal,
}

/// Fraction of recent samples per channel above this absolute value counts as
/// railed. Mirrors `crate::quality::RAILED_ABS_UV` (Cyton ADS1299 full scale).
const RAILED_WINDOW: usize = 128;
const RAILED_SAMPLE_FRAC: f32 = 0.5;
const RAILED_CHANNEL_FRAC: f32 = 0.5;

struct QualityVerdict {
    quality: SignalQuality,
    railed_channels: usize,
    assessed_channels: usize,
    hint: Option<String>,
}

fn assess_signal_quality(state: &EdgeState) -> QualityVerdict {
    if state.channel_buffers.is_empty()
        || state.channel_buffers.iter().all(|b| b.is_empty())
    {
        return QualityVerdict {
            quality: SignalQuality::NoSignal,
            railed_channels: 0,
            assessed_channels: 0,
            hint: Some(
                "无数据流：先 POST /connect 再 POST /start，并确认 OpenBCI 板已开机配对。"
                    .to_string(),
            ),
        };
    }

    let mut railed = 0usize;
    let mut assessed = 0usize;
    for buf in &state.channel_buffers {
        if buf.is_empty() {
            continue;
        }
        assessed += 1;
        let take = buf.len().min(RAILED_WINDOW);
        if take == 0 {
            continue;
        }
        let railed_n = buf
            .iter()
            .rev()
            .take(take)
            .filter(|v| v.abs() >= crate::quality::RAILED_ABS_UV)
            .count();
        if railed_n as f32 / take as f32 > RAILED_SAMPLE_FRAC {
            railed += 1;
        }
    }

    if assessed == 0 {
        return QualityVerdict {
            quality: SignalQuality::NoSignal,
            railed_channels: 0,
            assessed_channels: 0,
            hint: Some("尚无样本。".to_string()),
        };
    }

    if railed as f32 / assessed as f32 >= RAILED_CHANNEL_FRAC {
        return QualityVerdict {
            quality: SignalQuality::Railed,
            railed_channels: railed,
            assessed_channels: assessed,
            hint: Some(format!(
                "{railed}/{assessed} 通道饱和 (±187500µV ADC 满量程)：电极未接触头皮，\
                 或参考电极 BIAS/SRB 未接好。decision 不可信，先修电极接触。"
            )),
        };
    }

    QualityVerdict {
        quality: SignalQuality::Ok,
        railed_channels: railed,
        assessed_channels: assessed,
        hint: None,
    }
}

/// Single push frame delivered over `GET /events` (Server-Sent Events) so VR
/// / robot middlewares can consume the BCI state without polling.
#[derive(Debug, Clone, Serialize)]
pub struct EventFrame {
    pub t_unix: f64,
    pub connected: bool,
    pub streaming: bool,
    pub simulating: bool,
    pub sample_rate_hz: Option<f32>,
    pub eeg_channels: usize,
    /// Trust gate for `latest_decision`. See `SignalQuality`.
    pub signal_quality: SignalQuality,
    /// How many channels were pinned at the rail in the recent window.
    pub railed_channels: usize,
    /// Human-readable explanation when `signal_quality != Ok`.
    pub signal_hint: Option<String>,
    pub latest_decision: Option<SsvepDecision>,
    /// Latest µV sample, one row of length `eeg_channels`.
    pub last_sample: Option<Vec<f32>>,
    pub last_error: Option<String>,
}

struct Runtime {
    state: Arc<Mutex<EdgeState>>,
    config: EdgeConfig,
    session: Mutex<Option<OpenBciSession>>,
    stop_flag: Arc<AtomicBool>,
    worker: Mutex<Option<JoinHandle<()>>>,
    recorder: Arc<Mutex<Option<EdgeRecorder>>>,
    event_subs: Arc<Mutex<Vec<Sender<EventFrame>>>>,
}

impl Runtime {
    fn new(config: EdgeConfig) -> Self {
        Self {
            state: Arc::new(Mutex::new(EdgeState::default())),
            config,
            session: Mutex::new(None),
            stop_flag: Arc::new(AtomicBool::new(false)),
            worker: Mutex::new(None),
            recorder: Arc::new(Mutex::new(None)),
            event_subs: Arc::new(Mutex::new(Vec::new())),
        }
    }

    fn record_error(&self, err: impl Into<String>) {
        let mut s = self.state.lock().unwrap();
        s.status.last_error = Some(err.into());
    }
}

fn json_response<T: Serialize>(value: &T) -> Response<std::io::Cursor<Vec<u8>>> {
    let body = serde_json::to_vec(value).unwrap_or_else(|_| b"{\"ok\":false}".to_vec());
    let header = Header::from_bytes(&b"Content-Type"[..], &b"application/json"[..]).unwrap();
    let cors = Header::from_bytes(&b"Access-Control-Allow-Origin"[..], &b"*"[..]).unwrap();
    Response::from_data(body).with_header(header).with_header(cors)
}

fn build_event(state: &Arc<Mutex<EdgeState>>) -> EventFrame {
    let s = state.lock().unwrap();
    let last_sample = if !s.channel_buffers.is_empty()
        && !s.channel_buffers.iter().all(|b| b.is_empty())
    {
        Some(
            s.channel_buffers
                .iter()
                .map(|b| b.back().copied().unwrap_or(0.0))
                .collect(),
        )
    } else {
        None
    };
    let t_unix = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs_f64())
        .unwrap_or(0.0);

    let verdict = assess_signal_quality(&s);
    // When the headset is railed, never forward a confident decision —
    // it would just be FFT noise on a saturated rail. Downgrade the copy
    // we emit (stored state is untouched so /snapshot still shows raw µV).
    let latest_decision = match (&s.latest_decision, verdict.quality) {
        (Some(d), SignalQuality::Ok) => Some(d.clone()),
        (Some(d), _) => Some(SsvepDecision {
            best_freq_hz: None,
            confident: false,
            ..d.clone()
        }),
        (None, _) => None,
    };

    EventFrame {
        t_unix,
        connected: s.status.connected,
        streaming: s.status.streaming,
        simulating: s.status.simulating,
        sample_rate_hz: s.status.sample_rate_hz,
        eeg_channels: s.status.eeg_channels,
        signal_quality: verdict.quality,
        railed_channels: verdict.railed_channels,
        signal_hint: verdict.hint,
        latest_decision,
        last_sample,
        last_error: s.status.last_error.clone(),
    }
}

fn publish_event(
    state: &Arc<Mutex<EdgeState>>,
    subs: &Arc<Mutex<Vec<Sender<EventFrame>>>>,
) {
    let event = build_event(state);
    let mut guard = subs.lock().unwrap();
    // Drop subscribers whose receiver was closed (VR client disconnected).
    guard.retain(|tx| tx.send(event.clone()).is_ok());
}

/// Read source for SSE: serialises each `EventFrame` from the channel into
/// `data: <json>\n\n` chunks. tiny_http calls `read` repeatedly while the
/// HTTP/1.1 connection is open; returning Ok(0) terminates the response.
struct SseEventStream {
    rx: std::sync::mpsc::Receiver<EventFrame>,
    buffer: Vec<u8>,
    cursor: usize,
}

impl std::io::Read for SseEventStream {
    fn read(&mut self, buf: &mut [u8]) -> std::io::Result<usize> {
        if self.cursor >= self.buffer.len() {
            self.buffer.clear();
            self.cursor = 0;
            // Block until a new event is broadcast, with a periodic
            // keep-alive comment so proxies don't time out the connection.
            loop {
                match self.rx.recv_timeout(Duration::from_secs(15)) {
                    Ok(event) => {
                        let json = serde_json::to_string(&event)
                            .unwrap_or_else(|_| "{}".to_string());
                        self.buffer = format!("data: {json}\n\n").into_bytes();
                        break;
                    }
                    Err(RecvTimeoutError::Timeout) => {
                        // SSE comment line — ignored by clients but keeps the
                        // socket warm for any HTTP-aware proxy in the path.
                        self.buffer = b": keep-alive\n\n".to_vec();
                        break;
                    }
                    Err(RecvTimeoutError::Disconnected) => {
                        return Ok(0);
                    }
                }
            }
        }
        let n = buf.len().min(self.buffer.len() - self.cursor);
        buf[..n].copy_from_slice(&self.buffer[self.cursor..self.cursor + n]);
        self.cursor += n;
        Ok(n)
    }
}

fn handle_events(rt: &Arc<Runtime>, request: Request) -> std::io::Result<()> {
    let (tx, rx) = channel::<EventFrame>();
    // Send a snapshot frame immediately so a freshly connected client doesn't
    // wait up to 50ms for the first push.
    let _ = tx.send(build_event(&rt.state));
    rt.event_subs.lock().unwrap().push(tx);

    let stream = SseEventStream {
        rx,
        buffer: Vec::new(),
        cursor: 0,
    };
    let response = Response::new(
        StatusCode(200),
        vec![
            Header::from_bytes(&b"Content-Type"[..], &b"text/event-stream"[..]).unwrap(),
            Header::from_bytes(&b"Cache-Control"[..], &b"no-cache"[..]).unwrap(),
            Header::from_bytes(&b"Connection"[..], &b"keep-alive"[..]).unwrap(),
            Header::from_bytes(&b"X-Accel-Buffering"[..], &b"no"[..]).unwrap(),
            Header::from_bytes(&b"Access-Control-Allow-Origin"[..], &b"*"[..]).unwrap(),
        ],
        stream,
        None, // unknown content-length → chunked encoding
        None,
    );
    request.respond(response)
}

fn ok_response() -> Response<std::io::Cursor<Vec<u8>>> {
    json_response(&serde_json::json!({"ok": true}))
}

fn error_response(status: u16, message: &str) -> Response<std::io::Cursor<Vec<u8>>> {
    let body = serde_json::to_vec(&serde_json::json!({"ok": false, "error": message}))
        .unwrap_or_else(|_| b"{\"ok\":false}".to_vec());
    let header = Header::from_bytes(&b"Content-Type"[..], &b"application/json"[..]).unwrap();
    Response::from_data(body)
        .with_status_code(status)
        .with_header(header)
}

fn init_buffers(state: &Arc<Mutex<EdgeState>>, sample_rate_hz: f32, eeg_channels: usize) {
    let capacity = (sample_rate_hz * HISTORY_SECONDS).ceil() as usize;
    let labels: Vec<String> = CHANNEL_LABELS
        .iter()
        .take(eeg_channels)
        .map(|s| (*s).to_string())
        .collect();
    let mut s = state.lock().unwrap();
    s.buffer_capacity = capacity;
    s.channel_buffers = (0..eeg_channels)
        .map(|_| VecDeque::with_capacity(capacity))
        .collect();
    s.status.sample_rate_hz = Some(sample_rate_hz);
    s.status.eeg_channels = eeg_channels;
    s.status.channel_labels = labels;
    s.latest_decision = None;
}

fn push_sample(state: &Arc<Mutex<EdgeState>>, sample: &[f32]) {
    let mut s = state.lock().unwrap();
    let capacity = s.buffer_capacity;
    for (i, value) in sample.iter().enumerate() {
        if let Some(buf) = s.channel_buffers.get_mut(i) {
            if buf.len() == capacity {
                buf.pop_front();
            }
            buf.push_back(*value);
        }
    }
}

fn run_decoder(state: &Arc<Mutex<EdgeState>>, decoder: &SsvepDecoder) -> Option<SsvepDecision> {
    let snapshot: Vec<Vec<f32>> = {
        let s = state.lock().unwrap();
        let window = (decoder.config().sample_rate_hz * decoder.config().window_seconds) as usize;
        POSTERIOR_INDICES
            .iter()
            .filter_map(|&idx| s.channel_buffers.get(idx))
            .map(|buf| {
                let take = buf.len().min(window);
                buf.iter().rev().take(take).rev().cloned().collect()
            })
            .collect()
    };
    if snapshot.iter().any(|c| c.len() < 32) {
        return None;
    }
    let decision = decoder.decide(&snapshot);
    let mut s = state.lock().unwrap();
    s.latest_decision = Some(decision.clone());
    Some(decision)
}

fn record_sample(
    recorder: &Arc<Mutex<Option<EdgeRecorder>>>,
    t_sec: f32,
    channels: &[f32],
) {
    let mut guard = recorder.lock().unwrap();
    if let Some(rec) = guard.as_mut() {
        let _ = rec.write_sample(t_sec, channels);
    }
}

fn record_decision(
    recorder: &Arc<Mutex<Option<EdgeRecorder>>>,
    t_sec: f32,
    decision: &SsvepDecision,
) {
    let mut guard = recorder.lock().unwrap();
    if let Some(rec) = guard.as_mut() {
        let _ = rec.write_decision(t_sec, decision);
    }
}

fn handle_record_start(rt: &Arc<Runtime>) -> Response<std::io::Cursor<Vec<u8>>> {
    {
        let guard = rt.recorder.lock().unwrap();
        if guard.is_some() {
            return error_response(409, "recording already in progress");
        }
    }
    let (sample_rate_hz, channels) = {
        let s = rt.state.lock().unwrap();
        (
            s.status.sample_rate_hz.unwrap_or(SIMULATED_SAMPLE_RATE_HZ),
            if s.status.channel_labels.is_empty() {
                CHANNEL_LABELS.iter().map(|s| (*s).to_string()).collect()
            } else {
                s.status.channel_labels.clone()
            },
        )
    };
    let metadata = RecordingMetadata {
        board_id: rt.config.board_id,
        serial_port: rt.config.serial_port.clone(),
        sample_rate_hz,
        channels,
        target_freqs_hz: rt.config.target_freqs_hz(),
        created_at_unix: 0,
    };
    let data_dir = PathBuf::from(&rt.config.data_dir);
    match EdgeRecorder::start(&data_dir, metadata) {
        Ok(rec) => {
            let session_dir = rec.paths().session_dir.clone();
            *rt.recorder.lock().unwrap() = Some(rec);
            json_response(&serde_json::json!({
                "ok": true,
                "session_dir": session_dir.to_string_lossy(),
            }))
        }
        Err(err) => {
            rt.record_error(format!("record/start: {err}"));
            error_response(500, &format!("record/start failed: {err}"))
        }
    }
}

fn handle_record_stop(rt: &Arc<Runtime>) -> Response<std::io::Cursor<Vec<u8>>> {
    let rec = rt.recorder.lock().unwrap().take();
    match rec {
        Some(r) => {
            let dir = r.paths().session_dir.clone();
            if let Err(err) = r.close() {
                rt.record_error(format!("record/stop: {err}"));
                return error_response(500, &format!("record/stop failed: {err}"));
            }
            json_response(&serde_json::json!({
                "ok": true,
                "session_dir": dir.to_string_lossy(),
            }))
        }
        None => error_response(409, "no recording in progress"),
    }
}

fn spawn_worker_real(rt: &Arc<Runtime>, mut session: OpenBciSession) -> anyhow::Result<()> {
    let sample_rate_hz = session.sample_rate_hz();
    let eeg_channels = session.eeg_channel_count();
    init_buffers(&rt.state, sample_rate_hz, eeg_channels);

    session.start_stream()?;

    let state = rt.state.clone();
    let stop_flag = rt.stop_flag.clone();
    let recorder = rt.recorder.clone();
    let event_subs = rt.event_subs.clone();
    let target_freqs = rt.config.target_freqs_hz();
    let window_seconds = rt.config.window_sec;

    let handle = thread::spawn(move || {
        let decoder = SsvepDecoder::new(SsvepConfig {
            target_freqs_hz: target_freqs,
            sample_rate_hz,
            window_seconds,
            harmonics: 2,
        });
        let mut tick = 0u64;
        let decode_every = ((sample_rate_hz / 4.0).max(1.0)) as u64; // ~4 decisions/sec
        // Push state to /events subscribers ~20 Hz so VR sees fresh frames.
        let publish_every = ((sample_rate_hz / 20.0).max(1.0)) as u64;
        let max_batch_samples = ((sample_rate_hz / 10.0).ceil() as usize).clamp(1, 64);
        while !stop_flag.load(Ordering::SeqCst) {
            match session.drain_samples(max_batch_samples) {
                Ok(samples) if !samples.is_empty() => {
                    for values in samples {
                        let f32s: Vec<f32> = values.into_iter().map(|v| v as f32).collect();
                        let t_sec = tick as f32 / sample_rate_hz;
                        push_sample(&state, &f32s);
                        record_sample(&recorder, t_sec, &f32s);
                        tick += 1;
                        if tick % decode_every == 0 {
                            if let Some(decision) = run_decoder(&state, &decoder) {
                                record_decision(&recorder, t_sec, &decision);
                            }
                        }
                        if tick % publish_every == 0 {
                            publish_event(&state, &event_subs);
                        }
                    }
                }
                Ok(_) => thread::sleep(Duration::from_millis(2)),
                Err(err) => {
                    let mut s = state.lock().unwrap();
                    s.status.last_error = Some(format!("acquisition: {err}"));
                    drop(s);
                    thread::sleep(Duration::from_millis(50));
                }
            }
        }
        let _ = session.stop_stream();
        let _ = session.release();
        let mut s = state.lock().unwrap();
        s.status.streaming = false;
        s.status.connected = false;
    });
    *rt.worker.lock().unwrap() = Some(handle);
    {
        let mut s = rt.state.lock().unwrap();
        s.status.connected = true;
        s.status.streaming = true;
        s.status.last_error = None;
    }
    Ok(())
}

fn spawn_worker_simulated(rt: &Arc<Runtime>) {
    let eeg_channels = CHANNEL_LABELS.len();
    init_buffers(&rt.state, SIMULATED_SAMPLE_RATE_HZ, eeg_channels);

    let state = rt.state.clone();
    let stop_flag = rt.stop_flag.clone();
    let recorder = rt.recorder.clone();
    let event_subs = rt.event_subs.clone();
    let target_freqs = rt.config.target_freqs_hz();
    let window_seconds = rt.config.window_sec;

    let handle = thread::spawn(move || {
        let decoder = SsvepDecoder::new(SsvepConfig {
            target_freqs_hz: target_freqs,
            sample_rate_hz: SIMULATED_SAMPLE_RATE_HZ,
            window_seconds,
            harmonics: 2,
        });
        let dt = 1.0 / SIMULATED_SAMPLE_RATE_HZ;
        let started = Instant::now();
        let mut sample_index: u64 = 0;
        let decode_every = (SIMULATED_SAMPLE_RATE_HZ as u64 / 4).max(1);
        let publish_every = (SIMULATED_SAMPLE_RATE_HZ as u64 / 20).max(1);
        let tick_dur = Duration::from_secs_f32(dt);
        while !stop_flag.load(Ordering::SeqCst) {
            let t = sample_index as f32 * dt;
            let value = (2.0 * std::f32::consts::PI * SIMULATED_TARGET_HZ * t).sin();
            let sample: Vec<f32> = (0..eeg_channels).map(|_| value).collect();
            push_sample(&state, &sample);
            record_sample(&recorder, t, &sample);
            sample_index += 1;
            if sample_index % decode_every == 0 {
                if let Some(decision) = run_decoder(&state, &decoder) {
                    record_decision(&recorder, t, &decision);
                }
            }
            if sample_index % publish_every == 0 {
                publish_event(&state, &event_subs);
            }
            // Sleep based on wall clock so we don't drift.
            let target_elapsed = Duration::from_secs_f32(sample_index as f32 * dt);
            let now = started.elapsed();
            if target_elapsed > now {
                thread::sleep((target_elapsed - now).min(tick_dur));
            }
        }
        let mut s = state.lock().unwrap();
        s.status.streaming = false;
        s.status.connected = false;
    });
    *rt.worker.lock().unwrap() = Some(handle);
    {
        let mut s = rt.state.lock().unwrap();
        s.status.connected = true;
        s.status.streaming = true;
        s.status.simulating = true;
        s.status.last_error = None;
    }
}

fn handle_connect(rt: &Arc<Runtime>) -> Response<std::io::Cursor<Vec<u8>>> {
    if rt.config.simulate {
        let mut s = rt.state.lock().unwrap();
        s.status.connected = true;
        s.status.simulating = true;
        s.status.last_error = None;
        return ok_response();
    }
    {
        let s = rt.state.lock().unwrap();
        if s.status.streaming {
            return error_response(409, "already streaming");
        }
    }
    match OpenBciSession::connect_with_board_id(&rt.config.serial_port, rt.config.board_id) {
        Ok(session) => {
            let sample_rate_hz = session.sample_rate_hz();
            let eeg_channels = session.eeg_channel_count();
            *rt.session.lock().unwrap() = Some(session);
            init_buffers(&rt.state, sample_rate_hz, eeg_channels);
            let mut s = rt.state.lock().unwrap();
            s.status.connected = true;
            s.status.last_error = None;
            ok_response()
        }
        Err(err) => {
            rt.record_error(format!("connect: {err}"));
            error_response(500, &format!("connect failed: {err}"))
        }
    }
}

fn handle_start(rt: &Arc<Runtime>) -> Response<std::io::Cursor<Vec<u8>>> {
    {
        let s = rt.state.lock().unwrap();
        if s.status.streaming {
            return ok_response();
        }
    }
    rt.stop_flag.store(false, Ordering::SeqCst);
    if rt.config.simulate {
        spawn_worker_simulated(rt);
        return ok_response();
    }
    let session = match rt.session.lock().unwrap().take() {
        Some(s) => s,
        None => return error_response(409, "not connected"),
    };
    match spawn_worker_real(rt, session) {
        Ok(()) => ok_response(),
        Err(err) => {
            rt.record_error(format!("start: {err}"));
            error_response(500, &format!("start failed: {err}"))
        }
    }
}

fn handle_stop(rt: &Arc<Runtime>) -> Response<std::io::Cursor<Vec<u8>>> {
    rt.stop_flag.store(true, Ordering::SeqCst);
    let handle = rt.worker.lock().unwrap().take();
    if let Some(h) = handle {
        let _ = h.join();
    }
    let mut s = rt.state.lock().unwrap();
    s.status.streaming = false;
    s.status.simulating = false;
    s.status.connected = false;
    ok_response()
}

fn handle_status(rt: &Arc<Runtime>) -> Response<std::io::Cursor<Vec<u8>>> {
    let s = rt.state.lock().unwrap();
    json_response(&s.status)
}

fn handle_snapshot(rt: &Arc<Runtime>) -> Response<std::io::Cursor<Vec<u8>>> {
    let s = rt.state.lock().unwrap();
    let labels = s.status.channel_labels.clone();
    let sample_rate_hz = s.status.sample_rate_hz;
    let verdict = assess_signal_quality(&s);
    let channels: Vec<Vec<f32>> = s
        .channel_buffers
        .iter()
        .map(|buf| buf.iter().cloned().collect())
        .collect();
    json_response(&serde_json::json!({
        "sample_rate_hz": sample_rate_hz,
        "channel_labels": labels,
        "signal_quality": verdict.quality,
        "railed_channels": verdict.railed_channels,
        "signal_hint": verdict.hint,
        "channels": channels,
    }))
}

fn handle_decision(rt: &Arc<Runtime>) -> Response<std::io::Cursor<Vec<u8>>> {
    let s = rt.state.lock().unwrap();
    let verdict = assess_signal_quality(&s);
    // Mirror the SSE behaviour: do not hand back a confident pick on a
    // railed/absent signal.
    let decision = match (&s.latest_decision, verdict.quality) {
        (Some(d), SignalQuality::Ok) => Some(d.clone()),
        (Some(d), _) => Some(SsvepDecision {
            best_freq_hz: None,
            confident: false,
            ..d.clone()
        }),
        (None, _) => None,
    };
    json_response(&serde_json::json!({
        "decision": decision,
        "signal_quality": verdict.quality,
        "railed_channels": verdict.railed_channels,
        "signal_hint": verdict.hint,
    }))
}

fn route(rt: &Arc<Runtime>, request: Request) -> std::io::Result<()> {
    let path = request.url().split('?').next().unwrap_or("/").to_string();
    let method = request.method().as_str().to_string();

    // /events is a long-lived SSE stream — handle it on its own thread so the
    // main accept loop keeps serving short requests.
    if method == "GET" && path == "/events" {
        let rt2 = Arc::clone(rt);
        thread::spawn(move || {
            if let Err(err) = handle_events(&rt2, request) {
                eprintln!("/events stream closed: {err}");
            }
        });
        return Ok(());
    }
    if method == "OPTIONS" {
        // CORS preflight for browser-based VR clients.
        let resp = Response::from_string("")
            .with_status_code(204)
            .with_header(
                Header::from_bytes(&b"Access-Control-Allow-Origin"[..], &b"*"[..]).unwrap(),
            )
            .with_header(
                Header::from_bytes(
                    &b"Access-Control-Allow-Methods"[..],
                    &b"GET,POST,OPTIONS"[..],
                )
                .unwrap(),
            )
            .with_header(
                Header::from_bytes(&b"Access-Control-Allow-Headers"[..], &b"Content-Type"[..])
                    .unwrap(),
            );
        return request.respond(resp);
    }

    let response = match (method.as_str(), path.as_str()) {
        ("GET", "/health") => ok_response(),
        ("GET", "/status") => handle_status(rt),
        ("POST", "/connect") => handle_connect(rt),
        ("POST", "/start") => handle_start(rt),
        ("POST", "/stop") => handle_stop(rt),
        ("GET", "/snapshot") => handle_snapshot(rt),
        ("GET", "/decision") => handle_decision(rt),
        ("POST", "/record/start") => handle_record_start(rt),
        ("POST", "/record/stop") => handle_record_stop(rt),
        _ => error_response(404, "not found"),
    };
    request.respond(response)
}

pub fn run_edge_service(config: EdgeConfig) -> anyhow::Result<()> {
    let bind = format!("{}:{}", config.host, config.port);
    let server = Server::http(&bind).map_err(|e| anyhow::anyhow!("failed to bind {bind}: {e}"))?;
    let runtime = Arc::new(Runtime::new(config));
    println!("neurostick pi edge listening on http://{bind}");

    for request in server.incoming_requests() {
        if let Err(err) = route(&runtime, request) {
            eprintln!("request error: {err}");
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn state_with(channels: Vec<Vec<f32>>) -> EdgeState {
        let mut s = EdgeState::default();
        s.status.eeg_channels = channels.len();
        s.channel_buffers = channels.into_iter().map(|c| c.into_iter().collect()).collect();
        s
    }

    #[test]
    fn no_signal_when_buffers_empty() {
        let s = EdgeState::default();
        let v = assess_signal_quality(&s);
        assert_eq!(v.quality, SignalQuality::NoSignal);
        assert_eq!(v.railed_channels, 0);
        assert!(v.hint.is_some());
    }

    #[test]
    fn ok_for_plausible_microvolt_signal() {
        // 16 channels of small sinusoid-ish values, well under the rail.
        let channels: Vec<Vec<f32>> = (0..16)
            .map(|_| (0..200).map(|i| 20.0 * ((i as f32) * 0.1).sin()).collect())
            .collect();
        let s = state_with(channels);
        let v = assess_signal_quality(&s);
        assert_eq!(v.quality, SignalQuality::Ok);
        assert_eq!(v.railed_channels, 0);
        assert!(v.hint.is_none());
    }

    #[test]
    fn railed_when_channels_pinned_at_full_scale() {
        // Reproduces the colleague's Pi screenshot: every channel stuck at
        // the ADS1299 negative rail (-187500 µV).
        let channels: Vec<Vec<f32>> = (0..16)
            .map(|_| vec![-187_500.015_625_f32; 200])
            .collect();
        let s = state_with(channels);
        let v = assess_signal_quality(&s);
        assert_eq!(v.quality, SignalQuality::Railed);
        assert_eq!(v.railed_channels, 16);
        assert_eq!(v.assessed_channels, 16);
        assert!(v.hint.unwrap().contains("饱和"));
    }

    #[test]
    fn ok_when_only_a_minority_railed() {
        // 4 of 16 railed should still be usable (a couple of bad electrodes).
        let mut channels: Vec<Vec<f32>> = (0..12)
            .map(|_| (0..200).map(|i| 15.0 * ((i as f32) * 0.2).cos()).collect())
            .collect();
        for _ in 0..4 {
            channels.push(vec![187_500.0_f32; 200]);
        }
        let s = state_with(channels);
        let v = assess_signal_quality(&s);
        assert_eq!(v.quality, SignalQuality::Ok);
        assert_eq!(v.railed_channels, 4);
    }
}
