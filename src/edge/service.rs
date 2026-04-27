use std::collections::VecDeque;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::thread::{self, JoinHandle};
use std::time::{Duration, Instant};

use serde::Serialize;
use tiny_http::{Header, Request, Response, Server};

use crate::edge::config::EdgeConfig;
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

struct Runtime {
    state: Arc<Mutex<EdgeState>>,
    config: EdgeConfig,
    session: Mutex<Option<OpenBciSession>>,
    stop_flag: Arc<AtomicBool>,
    worker: Mutex<Option<JoinHandle<()>>>,
}

impl Runtime {
    fn new(config: EdgeConfig) -> Self {
        Self {
            state: Arc::new(Mutex::new(EdgeState::default())),
            config,
            session: Mutex::new(None),
            stop_flag: Arc::new(AtomicBool::new(false)),
            worker: Mutex::new(None),
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
    Response::from_data(body).with_header(header)
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

fn run_decoder(state: &Arc<Mutex<EdgeState>>, decoder: &SsvepDecoder) {
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
        return;
    }
    let decision = decoder.decide(&snapshot);
    let mut s = state.lock().unwrap();
    s.latest_decision = Some(decision);
}

fn spawn_worker_real(rt: &Arc<Runtime>, mut session: OpenBciSession) -> anyhow::Result<()> {
    let sample_rate_hz = session.sample_rate_hz();
    let eeg_channels = session.eeg_channel_count();
    init_buffers(&rt.state, sample_rate_hz, eeg_channels);

    session.start_stream()?;

    let state = rt.state.clone();
    let stop_flag = rt.stop_flag.clone();
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
        while !stop_flag.load(Ordering::SeqCst) {
            match session.next_sample() {
                Ok(Some(values)) => {
                    let f32s: Vec<f32> = values.into_iter().map(|v| v as f32).collect();
                    push_sample(&state, &f32s);
                    tick += 1;
                    if tick % decode_every == 0 {
                        run_decoder(&state, &decoder);
                    }
                }
                Ok(None) => thread::sleep(Duration::from_millis(2)),
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
        let tick_dur = Duration::from_secs_f32(dt);
        while !stop_flag.load(Ordering::SeqCst) {
            let t = sample_index as f32 * dt;
            let value = (2.0 * std::f32::consts::PI * SIMULATED_TARGET_HZ * t).sin();
            let sample: Vec<f32> = (0..eeg_channels).map(|_| value).collect();
            push_sample(&state, &sample);
            sample_index += 1;
            if sample_index % decode_every == 0 {
                run_decoder(&state, &decoder);
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
    let channels: Vec<Vec<f32>> = s
        .channel_buffers
        .iter()
        .map(|buf| buf.iter().cloned().collect())
        .collect();
    json_response(&serde_json::json!({
        "sample_rate_hz": sample_rate_hz,
        "channel_labels": labels,
        "channels": channels,
    }))
}

fn handle_decision(rt: &Arc<Runtime>) -> Response<std::io::Cursor<Vec<u8>>> {
    let s = rt.state.lock().unwrap();
    json_response(&serde_json::json!({
        "decision": s.latest_decision,
    }))
}

fn route(rt: &Arc<Runtime>, request: Request) -> std::io::Result<()> {
    let path = request.url().split('?').next().unwrap_or("/").to_string();
    let method = request.method().as_str().to_string();
    let response = match (method.as_str(), path.as_str()) {
        ("GET", "/health") => ok_response(),
        ("GET", "/status") => handle_status(rt),
        ("POST", "/connect") => handle_connect(rt),
        ("POST", "/start") => handle_start(rt),
        ("POST", "/stop") => handle_stop(rt),
        ("GET", "/snapshot") => handle_snapshot(rt),
        ("GET", "/decision") => handle_decision(rt),
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
