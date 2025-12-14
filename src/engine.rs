// src/engine.rs
use crate::drivers::{SignalBatch, SignalBuffer};
use crate::openbci::OpenBciSession;
use crate::recorder::DataRecorder;
use crate::types::*;
use crate::vjoy::VJoyClient;
use rustfft::{num_complex::Complex32, FftPlanner};
use std::collections::VecDeque;
use std::f64::consts::PI;
use std::sync::mpsc::{Receiver, Sender};
use std::thread;
use std::time::{Duration, Instant, SystemTime};

// =========================================================================
// 1. 内嵌 DSP 滤波器 (Biquad 实现) - 解决信号“脏”的问题
// =========================================================================
#[derive(Clone)]
struct Biquad {
    a0: f64, a1: f64, a2: f64,
    b0: f64, b1: f64, b2: f64,
    z1: f64, z2: f64,
}

impl Biquad {
    fn new_notch(fs: f64, freq: f64, q: f64) -> Self {
        let w0 = 2.0 * PI * freq / fs;
        let alpha = w0.sin() / (2.0 * q);
        let cos_w0 = w0.cos();
        let a0 = 1.0 + alpha;
        let b0 = 1.0;
        let b1 = -2.0 * cos_w0;
        let b2 = 1.0;
        let a1 = -2.0 * cos_w0;
        let a2 = 1.0 - alpha;
        Self {
            a0, a1, a2, b0, b1, b2, z1: 0.0, z2: 0.0,
        }
    }

    fn new_highpass(fs: f64, freq: f64, q: f64) -> Self {
        let w0 = 2.0 * PI * freq / fs;
        let alpha = w0.sin() / (2.0 * q);
        let cos_w0 = w0.cos();
        let a0 = 1.0 + alpha;
        let b0 = (1.0 + cos_w0) / 2.0;
        let b1 = -(1.0 + cos_w0);
        let b2 = (1.0 + cos_w0) / 2.0;
        let a1 = -2.0 * cos_w0;
        let a2 = 1.0 - alpha;
        Self {
            a0, a1, a2, b0, b1, b2, z1: 0.0, z2: 0.0,
        }
    }

    fn process(&mut self, input: f64) -> f64 {
        // Transposed Direct Form II to keep state in z1/z2
        let a1 = self.a1 / self.a0;
        let a2 = self.a2 / self.a0;
        let b0 = self.b0 / self.a0;
        let b1 = self.b1 / self.a0;
        let b2 = self.b2 / self.a0;

        let out = b0 * input + self.z1;
        self.z1 = b1 * input - a1 * out + self.z2;
        self.z2 = b2 * input - a2 * out;
        out
    }
}

// 修正后的 Filter 结构体
struct SimpleFilter {
    // 级联滤波器：先高通，再陷波
    hp: Vec<BiquadState>, // Per channel
    notch: Vec<BiquadState>, // Per channel
    fs: f64,
}

#[derive(Clone)]
struct BiquadState {
    x1: f64, x2: f64, y1: f64, y2: f64,
    b0: f64, b1: f64, b2: f64, a0: f64, a1: f64, a2: f64,
}

impl BiquadState {
    fn process(&mut self, x: f64) -> f64 {
        let y = (self.b0 * x + self.b1 * self.x1 + self.b2 * self.x2 
                 - self.a1 * self.y1 - self.a2 * self.y2) / self.a0;
        self.x2 = self.x1;
        self.x1 = x;
        self.y2 = self.y1;
        self.y1 = y;
        y
    }
}

impl SimpleFilter {
    fn new(channels: usize, fs: f64) -> Self {
        let mut hp = Vec::with_capacity(channels);
        let mut notch = Vec::with_capacity(channels);
        
        // 1. 3Hz 高通 (去漂移)
        let hp_coeffs = Self::calc_coeffs(fs, 3.0, 0.707, true);
        // 2. 50Hz 陷波 (去工频干扰 - 国内50Hz，如果是欧美改60Hz)
        let notch_coeffs = Self::calc_coeffs(fs, 50.0, 10.0, false);

        for _ in 0..channels {
            hp.push(hp_coeffs.clone());
            notch.push(notch_coeffs.clone());
        }
        Self { hp, notch, fs }
    }

    fn calc_coeffs(fs: f64, freq: f64, q: f64, is_highpass: bool) -> BiquadState {
        let w0 = 2.0 * PI * freq / fs;
        let alpha = w0.sin() / (2.0 * q);
        let cos_w0 = w0.cos();
        
        let (b0, b1, b2, a0, a1, a2) = if is_highpass {
            let a0 = 1.0 + alpha;
            (
                (1.0 + cos_w0) / 2.0, -(1.0 + cos_w0), (1.0 + cos_w0) / 2.0,
                a0, -2.0 * cos_w0, 1.0 - alpha
            )
        } else {
            // Notch
            let a0 = 1.0 + alpha;
            (
                1.0, -2.0 * cos_w0, 1.0,
                a0, -2.0 * cos_w0, 1.0 - alpha
            )
        };

        BiquadState { x1: 0.0, x2: 0.0, y1: 0.0, y2: 0.0, b0, b1, b2, a0, a1, a2 }
    }

    fn process_sample(&mut self, channel_idx: usize, sample: f64) -> f64 {
        if channel_idx >= self.hp.len() { return sample; }
        let s1 = self.hp[channel_idx].process(sample);
        self.notch[channel_idx].process(s1)
    }
}

// =========================================================================
// 2. 神经意图解码器 (逻辑判定)
// =========================================================================
fn process_neural_intent(
    data: &[f64],
    threshold: f64,
    calib_mode: bool,
    calib_max: &mut f64,
    start_time: Instant,
    tx: &Sender<BciMessage>,
) -> GamepadState {
    let mut gp = GamepadState::default();

    // 此时进来的 data 已经是滤波后的干净数据了
    let is_active = |idx: usize| -> bool { 
        data.get(idx).map(|&v| v.abs() > threshold).unwrap_or(false) 
    };
    let match_pattern = |indices: &[usize]| -> bool { indices.iter().all(|&i| is_active(i)) };

    // --- 游戏映射逻辑 (保持不变，但现在更准了) ---
    // 左摇杆 (WASD)
    if match_pattern(&[0, 4, 8]) { gp.ly += 1.0; } // W
    if match_pattern(&[1, 5, 9]) { gp.ly -= 1.0; } // S
    if match_pattern(&[2, 6, 10]) { gp.lx -= 1.0; } // A
    if match_pattern(&[3, 7, 11]) { gp.lx += 1.0; } // D

    // 动作键
    if match_pattern(&[0, 1, 2]) { gp.a = true; } 
    if match_pattern(&[3, 4, 5]) { gp.b = true; } 
    if match_pattern(&[6, 7, 8]) { gp.x = true; } 
    if match_pattern(&[9, 10, 11]) { gp.y = true; } 

    // 右摇杆 (IJKL)
    if match_pattern(&[12, 0]) { gp.ry += 1.0; }
    if match_pattern(&[13, 1]) { gp.ry -= 1.0; }
    if match_pattern(&[14, 2]) { gp.rx -= 1.0; }
    if match_pattern(&[15, 3]) { gp.rx += 1.0; }

    // 触发器/肩键
    if match_pattern(&[0, 15]) && gp.ry == 0.0 { gp.lb = true; }
    if match_pattern(&[2, 13]) && gp.rx == 0.0 { gp.rb = true; }
    if match_pattern(&[1, 14]) && gp.rx == 0.0 { gp.lt = true; }
    if match_pattern(&[3, 12]) && gp.ry == 0.0 { gp.rt = true; }

    // 校准逻辑
    if calib_mode {
        let max_s = data.iter().fold(0.0f64, |a, &b| a.max(b.abs()));
        if max_s > *calib_max {
            *calib_max = max_s;
        }
        if start_time.elapsed().as_secs() >= 3 {
            tx.send(BciMessage::CalibrationResult((), *calib_max)).ok();
        }
    }

    gp
}

fn hann_window(n: usize) -> Vec<f32> {
    if n == 0 {
        return Vec::new();
    }
    let denom = (n - 1).max(1) as f32;
    (0..n)
        .map(|i| 0.5 - 0.5 * (2.0 * std::f32::consts::PI * (i as f32) / denom).cos())
        .collect()
}

fn bandpower_fft(samples: &VecDeque<f64>, fs: f32, f_lo: f32, f_hi: f32) -> Option<f64> {
    let n = samples.len();
    if n < 64 || fs <= 0.0 || f_lo <= 0.0 || f_hi <= f_lo {
        return None;
    }

    let mut planner = FftPlanner::<f32>::new();
    let fft = planner.plan_fft_forward(n);
    let window = hann_window(n);

    let mut buffer: Vec<Complex32> = samples
        .iter()
        .copied()
        .zip(window.iter().copied())
        .map(|(v, w)| Complex32::new((v as f32) * w, 0.0))
        .collect();
    fft.process(&mut buffer);

    let hz_per_bin = fs / n as f32;
    let bin_lo = (f_lo / hz_per_bin).floor().max(0.0) as usize;
    let bin_hi = (f_hi / hz_per_bin).ceil().max(0.0) as usize;
    let max_bin = (n / 2).saturating_sub(1);

    let mut p = 0.0f64;
    for k in bin_lo..=bin_hi.min(max_bin) {
        let mag = buffer[k].norm() as f64;
        p += mag * mag;
    }
    Some(p / n as f64)
}

struct EegJoystickState {
    rest_mu_power: Option<f64>,
    act_mu_power: Option<f64>,
    calib_running: bool,
    calib_is_action: bool,
    calib_started: Instant,
    calib_sum: f64,
    calib_n: usize,
    ly_smooth: f32,
}

impl Default for EegJoystickState {
    fn default() -> Self {
        Self {
            rest_mu_power: None,
            act_mu_power: None,
            calib_running: false,
            calib_is_action: false,
            calib_started: Instant::now(),
            calib_sum: 0.0,
            calib_n: 0,
            ly_smooth: 0.0,
        }
    }
}

pub fn spawn_thread(tx: Sender<BciMessage>, rx_cmd: Receiver<GuiCommand>) {
    thread::spawn(move || {
        tx.send(BciMessage::Log("⚙️ Engine V14.0 (DSP Integrated)".to_owned())).ok();

        // --- 初始化 vJoy ---
        let mut joystick = VJoyClient::new(1).ok();
        if joystick.is_some() {
            tx.send(BciMessage::VJoyStatus(true)).ok();
        } else {
            tx.send(BciMessage::VJoyStatus(false)).ok();
            tx.send(BciMessage::Log("⚠️ vJoy not found. Gamepad disabled.".to_owned())).ok();
        }

        let mut recorder = DataRecorder::new();
        let mut openbci: Option<OpenBciSession> = None;
        let mut signal_buffer: Option<SignalBuffer> = None;
        
        // 默认采样率
        let mut current_sample_rate_hz: f32 = 250.0; 
        
        // --- 初始化 DSP 滤波器 ---
        let mut filters = SimpleFilter::new(16, current_sample_rate_hz as f64);

        let mut current_mode = ConnectionMode::Simulation;
        let mut is_active = false;
        let mut is_streaming = false;
        let mut threshold = 150.0; // 默认阈值稍微调低，因为去了直流

        let mut sim_phase: f64 = 0.0;
        let mut current_sim_input = SimInputIntent::default();
        let mut calib_mode = false;
        let mut calib_max_val = 0.0;
        let mut calib_start_time = Instant::now();

        // Channel labels based on OpenBCI 16ch 10-20 montage (user follows official wiring).
        // This also lets us refer to channels by semantic names like C3/C4.
        let montage_labels: Vec<String> = vec![
            "FP1", "FP2", "C3", "C4", "P7", "P8", "O1", "O2", "F7", "F8", "F3", "F4", "T7",
            "T8", "P3", "P4",
        ]
        .into_iter()
        .map(|s| s.to_string())
        .collect();
        let ch_c3 = montage_labels.iter().position(|s| s == "C3").unwrap_or(2);
        let ch_c4 = montage_labels.iter().position(|s| s == "C4").unwrap_or(3);
        let mut c3_buf: VecDeque<f64> = VecDeque::with_capacity(256);
        let mut c4_buf: VecDeque<f64> = VecDeque::with_capacity(256);
        let mut eeg_joy = EegJoystickState::default();

        // 缓存区
        let mut raw_channel_data = vec![0.0f64; 16];
        let mut clean_channel_data = vec![0.0f64; 16];
        let mut hw_sample_logged = false;

        // 循环控制
        let mut last_vjoy_update = Instant::now();
        let mut last_ui_frame_sent = Instant::now();
        let ui_frame_interval = Duration::from_millis(33); // ~30 FPS snapshots

        loop {
            // 1. 处理 GUI 命令 (非阻塞)
            while let Ok(cmd) = rx_cmd.try_recv() {
                match cmd {
                    GuiCommand::Connect(mode, port) => {
                        current_mode = mode;
                        if mode == ConnectionMode::Hardware {
                            match OpenBciSession::connect(&port) {
                                Ok(session) => {
                                    current_sample_rate_hz = session.sample_rate_hz();
                                    // 重置滤波器以匹配新采样率
                                    filters = SimpleFilter::new(16, current_sample_rate_hz as f64);
                                    openbci = Some(session);
                                    is_active = true;
                                    tx.send(BciMessage::Status(true)).ok();
                                    tx.send(BciMessage::Log(format!("✅ OpenBCI Connected ({} Hz)", current_sample_rate_hz))).ok();
                                }
                                Err(e) => { tx.send(BciMessage::Log(format!("❌ Failed: {}", e))).ok(); }
                            }
                        } else {
                            is_active = true;
                            tx.send(BciMessage::Status(true)).ok();
                            tx.send(BciMessage::Log("✅ Simulation Mode".to_owned())).ok();
                        }
                    }
                    GuiCommand::Disconnect => {
                        is_active = false; is_streaming = false;
                        openbci = None;
                        tx.send(BciMessage::Status(false)).ok();
                    }
                    GuiCommand::StartStream => { if is_active { 
                        is_streaming = true; 
                        if let Some(s) = openbci.as_mut() { s.start_stream().ok(); }
                        tx.send(BciMessage::Log("🌊 Stream Started".to_owned())).ok();
                    }}
                    GuiCommand::StopStream => { 
                        is_streaming = false; 
                        if let Some(s) = openbci.as_mut() { s.stop_stream().ok(); }
                        tx.send(BciMessage::Log("🛑 Stream Stopped".to_owned())).ok();
                    }
                    GuiCommand::SetThreshold(v) => threshold = v,
                    GuiCommand::StartCalibration(is_action) => {
                        // Repurpose calibration to compute µ-band power baseline for EEG->joystick.
                        eeg_joy.calib_running = true;
                        eeg_joy.calib_is_action = is_action;
                        eeg_joy.calib_started = Instant::now();
                        eeg_joy.calib_sum = 0.0;
                        eeg_joy.calib_n = 0;

                        // Keep legacy flags for GUI countdown/progress.
                        calib_mode = true;
                        calib_max_val = 0.0;
                        calib_start_time = Instant::now();
                    }
                    GuiCommand::UpdateSimInput(input) => current_sim_input = input,
                    GuiCommand::StartRecording(l) => { recorder.start(&l); tx.send(BciMessage::RecordingStatus(true)).ok(); }
                    GuiCommand::StopRecording => { recorder.stop(); tx.send(BciMessage::RecordingStatus(false)).ok(); }
                    _ => {}
                }
            }

            // 2. 数据采集与处理
            if is_streaming {
                let mut has_new_data = false;

                if current_mode == ConnectionMode::Simulation {
                    // 模拟数据生成
                    sim_phase += 0.1;
                    let noise = (sim_phase * 0.5).sin() * 5.0; // 模拟一些底噪
                    
                    raw_channel_data.fill(0.0);
                    // ... (此处省略太长的模拟输入判定，保持原样即可，重点是后面)
                    // 为了演示简单，这里只保留一部分模拟逻辑
                    let mut bump = |idx: usize| {
                        if let Some(v) = raw_channel_data.get_mut(idx) {
                            *v += 500.0;
                        }
                    };

                    // WASD -> Left stick
                    if current_sim_input.w { for &i in &[0, 4, 8] { bump(i); } }
                    if current_sim_input.s { for &i in &[1, 5, 9] { bump(i); } }
                    if current_sim_input.a { for &i in &[2, 6, 10] { bump(i); } }
                    if current_sim_input.d { for &i in &[3, 7, 11] { bump(i); } }

                    // Buttons
                    if current_sim_input.space { for &i in &[0, 1, 2] { bump(i); } } // A
                    if current_sim_input.key_z { for &i in &[3, 4, 5] { bump(i); } } // B
                    if current_sim_input.key_x { for &i in &[6, 7, 8] { bump(i); } } // X
                    if current_sim_input.key_c { for &i in &[9, 10, 11] { bump(i); } } // Y
                    
                    // 模拟模式也加上一点随机漂移，测试滤波器
                    for v in raw_channel_data.iter_mut() { *v += noise; }
                    
                    has_new_data = true;
                    thread::sleep(Duration::from_millis(4)); // 250Hz approx
                } else if let Some(session) = openbci.as_mut() {
                    match session.next_sample() {
                        Ok(Some(sample)) => {
                            for (i, v) in sample.iter().take(16).enumerate() {
                                raw_channel_data[i] = *v;
                            }
                            if !hw_sample_logged {
                                let first_vals: Vec<f64> = sample.iter().take(16).cloned().collect();
                                tx.send(BciMessage::Log(format!(
                                    "HW sample len={} first16={:?}",
                                    sample.len(),
                                    first_vals
                                )))
                                .ok();
                                hw_sample_logged = true;
                            }
                            has_new_data = true;
                        }
                        Ok(None) => {
                            // 没有数据时短暂休眠，避免死循环烧CPU
                            // 关键优化：休眠时间要极短
                            thread::sleep(Duration::from_micros(500)); 
                        }
                        Err(_) => { thread::sleep(Duration::from_millis(10)); }
                    }
                }

                if has_new_data {
                    // === 关键步骤：实时滤波 ===
                    // OpenBCI 的原始数据可能有几万的直流偏置，必须滤掉
                    for i in 0..16 {
                        let filtered = filters.process_sample(i, raw_channel_data[i]);
                        // BrainFlow 返回的 Cyton 数据是伏特级别，UI/阈值逻辑使用微伏，统一缩放
                        clean_channel_data[i] = if current_mode == ConnectionMode::Hardware {
                            filtered * 1e6 * 2.0 // 额外放大一倍，便于观察
                        } else {
                            filtered
                        };
                    }

                    // 录制原始数据(Raw)还是干净数据(Clean)? 
                    // 建议录制 Raw，方便以后调整算法。但为了演示效果，这里我们把 Clean 发给 UI
                    // Maintain EEG rolling window for µ-band power (C3/C4).
                    if current_mode == ConnectionMode::Hardware {
                        let push_fixed = |buf: &mut VecDeque<f64>, v: f64| {
                            if buf.len() == buf.capacity() {
                                buf.pop_front();
                            }
                            buf.push_back(v);
                        };
                        push_fixed(&mut c3_buf, clean_channel_data.get(ch_c3).copied().unwrap_or(0.0));
                        push_fixed(&mut c4_buf, clean_channel_data.get(ch_c4).copied().unwrap_or(0.0));
                    }

                    if recorder.is_recording() {
                        recorder.write_record(&raw_channel_data);
                    }

                    // === 发送数据给 UI 渲染 ===
                    // 初始化 Buffer (如果为空)
                    if signal_buffer.is_none() {
                        signal_buffer = SignalBuffer::with_history_seconds(
                            montage_labels.clone(),
                            current_sample_rate_hz,
                            10.0,
                        )
                        .ok();
                    }

                    if let Some(buf) = signal_buffer.as_mut() {
                        // 把 clean_channel_data 包装成 Batch
                        let batch = SignalBatch {
                            started_at: SystemTime::now(),
                            sample_rate_hz: current_sample_rate_hz,
                            channel_labels: buf.channel_labels().to_vec(),
                            samples: clean_channel_data.iter().map(|&v| vec![v as f32]).collect(),
                        };
                        buf.push_batch(&batch).ok();
                        
                        // 降低 UI 刷新频率，比如每 4 个采样发一次 GUI，或者只发最新的 snapshot
                        // 为了流畅度，这里每次都发，但 GUI 端要注意性能
                        if last_ui_frame_sent.elapsed() >= ui_frame_interval {
                            tx.send(BciMessage::DataFrame(buf.snapshot(5.0))).ok();
                            last_ui_frame_sent = Instant::now();
                        }
                    }

                    // === 神经解码 (使用干净数据) ===
                    let mut gp = GamepadState::default();
                    if current_mode == ConnectionMode::Hardware {
                        let fs = current_sample_rate_hz;
                        let mu_c3 = bandpower_fft(&c3_buf, fs, 8.0, 12.0);
                        let mu_c4 = bandpower_fft(&c4_buf, fs, 8.0, 12.0);
                        let mu = match (mu_c3, mu_c4) {
                            (Some(a), Some(b)) => Some((a + b) / 2.0),
                            (Some(a), None) => Some(a),
                            (None, Some(b)) => Some(b),
                            _ => None,
                        };

                        if eeg_joy.calib_running {
                            if let Some(p) = mu {
                                eeg_joy.calib_sum += p;
                                eeg_joy.calib_n += 1;
                            }
                            if eeg_joy.calib_started.elapsed().as_secs_f32() >= 3.0 {
                                eeg_joy.calib_running = false;
                                let avg = if eeg_joy.calib_n > 0 {
                                    eeg_joy.calib_sum / eeg_joy.calib_n as f64
                                } else {
                                    0.0
                                };
                                if eeg_joy.calib_is_action {
                                    eeg_joy.act_mu_power = Some(avg);
                                } else {
                                    eeg_joy.rest_mu_power = Some(avg);
                                }
                                tx.send(BciMessage::CalibrationResult((), avg)).ok();
                                calib_mode = false;
                            }
                        }

                        if let (Some(rest), Some(act), Some(cur)) =
                            (eeg_joy.rest_mu_power, eeg_joy.act_mu_power, mu)
                        {
                            let denom = (rest - act).abs().max(1e-9);
                            let mut norm = ((rest - cur) / denom) as f32;
                            norm = norm.clamp(0.0, 1.0);
                            let alpha = 0.18;
                            eeg_joy.ly_smooth = (1.0 - alpha) * eeg_joy.ly_smooth + alpha * norm;
                            gp.ly = if eeg_joy.ly_smooth < 0.08 { 0.0 } else { eeg_joy.ly_smooth };
                        }
                    } else {
                        gp = process_neural_intent(
                            &clean_channel_data,
                            threshold,
                            calib_mode,
                            &mut calib_max_val,
                            calib_start_time,
                            &tx,
                        );
                    }

                    // === 驱动 vJoy ===
                    // 只有当状态发生改变 或 每隔一定时间才更新，减少系统调用开销
                    // 这里为了响应速度，每帧都更新
                    if let Some(joy) = &mut joystick {
                        joy.set_button(1, gp.a);
                        joy.set_button(2, gp.b);
                        joy.set_button(3, gp.x);
                        joy.set_button(4, gp.y);
                        joy.set_button(5, gp.lb);
                        joy.set_button(6, gp.rb);
                        joy.set_button(7, gp.lt);
                        joy.set_button(8, gp.rt);
                        joy.set_button(9, gp.dpad_up);
                        joy.set_button(10, gp.dpad_down);
                        joy.set_button(11, gp.dpad_left);
                        joy.set_button(12, gp.dpad_right);
                        let axis = |v: f32| -> i32 {
                            let v = v.clamp(-1.0, 1.0) as f64;
                            (16384.0 + v * 16000.0) as i32
                        };
                        joy.set_axis(0x30, axis(gp.lx));
                        joy.set_axis(0x31, axis(gp.ly));
                        joy.set_axis(0x33, axis(gp.rx));
                        joy.set_axis(0x34, axis(gp.ry));
                        // ... 其他按键映射同理
                    }
                    
                    // 发送手柄状态给 UI 显示
                    if last_vjoy_update.elapsed().as_millis() > 30 {
                        tx.send(BciMessage::GamepadUpdate(gp)).ok();
                        last_vjoy_update = Instant::now();
                    }
                }
            } else {
                // 未推流时，降低 CPU 占用
                thread::sleep(Duration::from_millis(50));
            }
        }
    });
}
