#[derive(Clone, Copy, Debug)]
pub enum YScale {
    /// Match the Processing implementation: compute the min and max of the visible window
    /// once per refresh and use that as the Y axis range.
    Auto,
    /// Fix the Y axis to +/- the provided microvolt value.
    FixedMicrovolts(f32),
}
impl Default for YScale {
    fn default() -> Self {
        // Mirrors the OpenBCI GUI default of +/- 200 uV.
        YScale::FixedMicrovolts(200.0)
    }
}
#[derive(Clone, Copy, Debug)]
pub struct TimeWindow {
    pub seconds: f32,
}
impl TimeWindow {
    pub fn new(seconds: f32) -> Self {
        Self {
            seconds: seconds.max(0.1),
        }
    }
    pub fn samples(&self, sample_rate_hz: f32) -> usize {
        ((self.seconds * sample_rate_hz).ceil() as usize).max(1)
    }
}
impl Default for TimeWindow {
    fn default() -> Self {
        // Processing widget starts at 5 seconds.
        TimeWindow { seconds: 5.0 }
    }
}
