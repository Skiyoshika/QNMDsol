#[derive(Clone, Copy, Debug)]
pub struct SamplePoint {
    /// Absolute time in seconds.
    pub time: f32,
    /// Value in microvolts after filtering.
    pub value: f32,
}
#[derive(Debug)]
pub struct ChannelView {
    pub index: usize,
    pub y_range: (f32, f32),
    pub rms_u_v: f32,
    pub min: f32,
    pub max: f32,
    pub samples: Vec<SamplePoint>,
}
#[derive(Debug)]
pub struct WaveformView {
    pub window_secs: f32,
    pub channels: Vec<ChannelView>,
}
