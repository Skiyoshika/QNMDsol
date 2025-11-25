// src/engine.rs
use crate::types::*;
use crate::vjoy::VJoyClient;
use crate::recorder::DataRecorder;
use libloading::{Library, Symbol};
use std::ffi::CString;
use std::sync::mpsc::{Receiver, Sender};
use std::thread;
use std::time::{Duration, Instant};

pub fn spawn_thread(tx: Sender<BciMessage>, rx_cmd: Receiver<GuiCommand>) {
    thread::spawn(move || {
        // 发送启动日志
        tx.send(BciMessage::Log("⚙️ Core Engine v7.3 Ready.".to_owned())).ok();
        
        // 1. 初始化 vJoy
        let mut joystick = match VJoyClient::new(1) {
            Ok(j) => { tx.send(BciMessage::VJoyStatus(true)).ok(); Some(j) },
            Err(_) => { tx.send(BciMessage::VJoyStatus(false)).ok(); None }
        };

        // 2. 初始化数据录制器
        let mut recorder = DataRecorder::new();

        // 3. 加载 DLL
        let lib_opt = unsafe { Library::new("BoardController.dll").ok() };
        
        // 内部状态
        let mut current_mode = ConnectionMode::Simulation;
        let mut is_active = false;
        let mut is_streaming = false;
        let mut threshold = 200.0;
        
        // 模拟状态
        let mut sim_phase = 0.0;
        let mut current_sim_input = SimInputIntent::default();
        
        // 校准状态
        let mut calib_mode = false;
        let mut calib_max_val = 0.0;
        let mut calib_start = Instant::now();

        loop {
            // ============================================================
            // 1. 消息处理 (处理 GUI 发来的命令)
            // ============================================================
            for _ in 0..10 { 
                if let Ok(cmd) = rx_cmd.try_recv() {
                    match cmd {
                        GuiCommand::Connect(mode) => {
                            if !is_active {
                                current_mode = mode;
                                if mode == ConnectionMode::Simulation {
                                    is_active = true;
                                    tx.send(BciMessage::Status(true)).ok();
                                    tx.send(BciMessage::Log("✅ Sim Connected".to_owned())).ok();
                                } else if let Some(lib) = &lib_opt {
                                    // 硬件连接逻辑
                                    unsafe {
                                        let prepare: Symbol<unsafe extern "C" fn(i32, *const i8) -> i32> = lib.get(b"prepare_session").unwrap();
                                        let p_str = r#"{"serial_port":"COM4","timeout":3,"master_board":-100,"file":"","file_anc":"","file_aux":"","ip_address":"","ip_address_anc":"","ip_address_aux":"","ip_port":0,"ip_port_anc":0,"ip_port_aux":0,"ip_protocol":0,"mac_address":"","other_info":"","serial_number":""}"#;
                                        let p_cstr = CString::new(p_str).unwrap();
                                        
                                        // 尝试连接 ID 2 (Cyton+Daisy)
                                        if prepare(2, p_cstr.as_ptr()) == 0 {
                                            is_active = true;
                                            tx.send(BciMessage::Status(true)).ok();
                                            tx.send(BciMessage::Log("✅ Hardware Connected".to_owned())).ok();
                                        } else {
                                            tx.send(BciMessage::Log("❌ Connect Failed (Check Power/USB)".to_owned())).ok();
                                        }
                                    }
                                }
                            }
                        },
                        GuiCommand::Disconnect => { 
                            is_active = false; is_streaming = false; 
                            if recorder.is_recording() { recorder.stop(); tx.send(BciMessage::RecordingStatus(false)).ok(); }
                            tx.send(BciMessage::Status(false)).ok(); 
                            // (可选：调用 release_session)
                        },
                        GuiCommand::StartStream => { 
                            if is_active { 
                                is_streaming = true; 
                                if current_mode == ConnectionMode::Hardware {
                                    if let Some(lib) = &lib_opt {
                                        unsafe {
                                            let start: Symbol<unsafe extern "C" fn(i32, *const i8) -> i32> = lib.get(b"start_stream").unwrap();
                                            let empty = CString::new("").unwrap();
                                            start(45000, empty.as_ptr());
                                        }
                                    }
                                }
                                tx.send(BciMessage::Log("🌊 Stream Started".to_owned())).ok(); 
                            } 
                        },
                        GuiCommand::StopStream => { 
                            is_streaming = false; 
                            if current_mode == ConnectionMode::Hardware {
                                if let Some(lib) = &lib_opt {
                                    unsafe {
                                        let stop: Symbol<unsafe extern "C" fn(i32) -> i32> = lib.get(b"stop_stream").unwrap();
                                        stop(2);
                                    }
                                }
                            }
                            tx.send(BciMessage::Log("🛑 Stream Stopped".to_owned())).ok(); 
                        },
                        GuiCommand::SetThreshold(v) => threshold = v,
                        GuiCommand::StartCalibration(_) => { calib_mode = true; calib_max_val = 0.0; calib_start = Instant::now(); },
                        GuiCommand::UpdateSimInput(input) => current_sim_input = input,
                        GuiCommand::StartRecording(label) => { recorder.start(&label); tx.send(BciMessage::RecordingStatus(true)).ok(); },
                        GuiCommand::StopRecording => { recorder.stop(); tx.send(BciMessage::RecordingStatus(false)).ok(); }
                    }
                } else {
                    break; 
                }
            }

            // ============================================================
            // 2. 数据流循环
            // ============================================================
            if is_streaming {
                let mut channel_data = vec![0.0f64; 16];

                // --- 分支 A: 模拟模式 ---
                if current_mode == ConnectionMode::Simulation {
                    sim_phase += 0.1;
                    for i in 0..16 { channel_data[i] = (sim_phase * (i as f64 * 0.1 + 1.0)).sin() * 2.0; }
                    
                    let amp = 1000.0;
                    if current_sim_input.w { channel_data[0] += amp; }
                    if current_sim_input.s { channel_data[1] += amp; }
                    if current_sim_input.a { channel_data[2] += amp; }
                    if current_sim_input.d { channel_data[3] += amp; }
                    if current_sim_input.space { channel_data[4] += amp; } 
                    if current_sim_input.key_z { channel_data[5] += amp; } 
                    if current_sim_input.key_x { channel_data[6] += amp; } 
                    if current_sim_input.key_c { channel_data[7] += amp; } 
                    if current_sim_input.up    { channel_data[8] += amp; } 
                    if current_sim_input.down  { channel_data[9] += amp; } 
                    if current_sim_input.left  { channel_data[10] += amp; } 
                    if current_sim_input.right { channel_data[11] += amp; } 
                    
                    thread::sleep(Duration::from_millis(5));
                } 
                // --- 分支 B: 硬件模式 ---
                else if let Some(lib) = &lib_opt {
                    unsafe {
                        let get_cnt: Symbol<unsafe extern "C" fn(i32, *mut i32) -> i32> = lib.get(b"get_board_data_count").unwrap();
                        let get_dat: Symbol<unsafe extern "C" fn(i32, *mut f64) -> i32> = lib.get(b"get_board_data").unwrap();
                        let get_row: Symbol<unsafe extern "C" fn(i32, *mut i32) -> i32> = lib.get(b"get_num_rows").unwrap();
                        
                        let mut count = 0; 
                        get_cnt(2, &mut count);
                        
                        if count > 0 {
                            let mut rows = 0; 
                            get_row(2, &mut rows);
                            let mut buf = vec![0.0f64; (rows * count) as usize];
                            get_dat(count, buf.as_mut_ptr());
                            
                            // 获取最新一个采样点的数据填充到 channel_data
                            // 我们只需要最新的一个点来做实时控制
                            for i in 0..count {
                                // === 🔴 修复点：强制类型转换为 usize ===
                                let current_sample_index = i as usize; 
                                
                                // 假设前 16 个通道是 EEG 数据 (对于 Cyton+Daisy 确实如此，通常是 ch 1-16)
                                // BrainFlow 的数据通常是一维数组：[ch0_data..., ch1_data..., ch2_data...]
                                // 或者是 [sample0_all_chs, sample1_all_chs...] 
                                // BrainFlow C API get_board_data 返回的是 Column-Major 还是 Row-Major 取决于具体实现
                                // 但通常是一行一行的数据。
                                // 为了保险，我们这里做一个简单的映射测试，假设 rows 是通道数
                                
                                for c in 0..16 {
                                    // 计算索引：行号(c + 1) * 总点数 + 当前点(i)
                                    // 注意：Cyton 的 EEG 数据通常从第 1 行开始 (第 0 行是时间戳/包序号)
                                    let row_idx = (c + 1) as usize; 
                                    let idx = row_idx * (count as usize) + current_sample_index;
                                    
                                    if idx < buf.len() {
                                        channel_data[c] = buf[idx];
                                    }
                                }
                            }
                        }
                    }
                    thread::sleep(Duration::from_millis(5));
                }

                // 3. 录制数据
                if recorder.is_recording() { recorder.write_record(&channel_data); }

                // 4. 信号处理 (阈值判断)
                let mut gp = GamepadState::default();
                
                // 左摇杆
                if channel_data[0].abs() > threshold { gp.ly += 1.0; } 
                if channel_data[1].abs() > threshold { gp.ly -= 1.0; } 
                if channel_data[2].abs() > threshold { gp.lx -= 1.0; } 
                if channel_data[3].abs() > threshold { gp.lx += 1.0; } 

                // 按键
                if channel_data[4].abs() > threshold { gp.a = true; }
                if channel_data[5].abs() > threshold { gp.b = true; }
                if channel_data[6].abs() > threshold { gp.x = true; }
                if channel_data[7].abs() > threshold { gp.y = true; }
                
                // 右摇杆
                if channel_data[8].abs() > threshold { gp.ry += 1.0; }
                if channel_data[9].abs() > threshold { gp.ry -= 1.0; }
                if channel_data[10].abs() > threshold { gp.rx -= 1.0; }
                if channel_data[11].abs() > threshold { gp.rx += 1.0; }

                // 校准逻辑
                if calib_mode {
                    let max_s = channel_data.iter().fold(0.0f64, |a, &b| a.max(b.abs()));
                    if max_s > calib_max_val { calib_max_val = max_s; }
                    if calib_start.elapsed().as_secs() >= 3 {
                        calib_mode = false;
                        tx.send(BciMessage::CalibrationResult((), calib_max_val)).ok();
                    }
                }

                // 5. vJoy 输出
                if let Some(joy) = &mut joystick {
                    joy.set_button(1, gp.a);
                    joy.set_button(2, gp.b);
                    joy.set_button(3, gp.x);
                    joy.set_button(4, gp.y);
                    
                    let to_axis = |v: f32| (16384.0 + v * 16000.0) as i32;
                    joy.set_axis(0x30, to_axis(gp.lx)); // X
                    joy.set_axis(0x31, to_axis(gp.ly)); // Y
                    joy.set_axis(0x32, to_axis(gp.rx)); // Z
                    joy.set_axis(0x33, to_axis(gp.ry)); // Rx
                }

                // 6. 发送反馈 (降频)
                if sim_phase as i32 % 2 == 0 {
                    tx.send(BciMessage::GamepadUpdate(gp)).ok();
                    tx.send(BciMessage::DataPacket(channel_data)).ok();
                }

            } else {
                thread::sleep(Duration::from_millis(50));
            }
        }
    });
}