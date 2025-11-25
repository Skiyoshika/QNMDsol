// src/types.rs

// 连接模式
#[derive(PartialEq, Clone, Copy, Debug)]
pub enum ConnectionMode {
    Simulation,
    Hardware,
}

// GUI 发给后台的命令
#[derive(Clone, Debug)]
pub enum GuiCommand {
    Connect(ConnectionMode),
    Disconnect,
    StartStream,
    StopStream,
    SetThreshold(f64),
    // 模拟输入状态更新
    UpdateSimInput(SimInputIntent),
    // AI 数据录制命令
    StartRecording(String), 
    StopRecording,
    // === 修复：补回校准命令 ===
    StartCalibration(bool),
}

// 后台发给 GUI 的消息
#[derive(Clone, Debug)]
pub enum BciMessage {
    Log(String),
    Status(bool),       // 连接状态
    VJoyStatus(bool),   // vJoy 状态
    DataPacket(Vec<f64>), // 绘图数据
    GamepadUpdate(GamepadState), // 手柄状态
    RecordingStatus(bool), // 录制状态
    // === 修复：补回校准结果 ===
    CalibrationResult((), f64), 
}

// 手柄状态结构体
#[derive(Clone, Copy, Debug, Default)]
pub struct GamepadState {
    pub lx: f32, pub ly: f32,
    pub rx: f32, pub ry: f32,
    pub a: bool, pub b: bool, pub x: bool, pub y: bool,
    pub lb: bool, pub rb: bool,
}

// 模拟输入意图
#[derive(Default, Clone, Copy, Debug)]
pub struct SimInputIntent {
    pub w: bool, pub a: bool, pub s: bool, pub d: bool,
    pub up: bool, pub down: bool, pub left: bool, pub right: bool,
    pub space: bool, pub key_z: bool, pub key_x: bool, pub key_c: bool,
    pub key_1: bool, pub key_2: bool,
}