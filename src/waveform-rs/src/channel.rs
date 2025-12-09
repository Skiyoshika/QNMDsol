use std::time::Instant;
use super::{
    buffer::SampleBuffer,
    config::{TimeWindow, YScale},
    filter::{FilterChain, FilterKind},
    view::{ChannelView, SamplePoint, WaveformView},
};
#[derive(Clone, Debug)]
pub struct ChannelConfig {
    pub index: usize,
    pub enabled: bool,
    pub y_scale: YScale,
    pub filters: Vec<FilterKind>,
}
impl ChannelConfig {
    pub fn new(index: usize) -> Self {
        Self {
            index,
            enabled: true,
            y_scale: YScale::default(),
            filters: Vec::new(),
        }
    }
}
struct ChannelState {
    config: ChannelConfig,
    buffer: SampleBuffer,
    filters: FilterChain,
    last_sample_time: f32,
}
impl ChannelState {
    fn new(config: ChannelConfig, time_window: TimeWindow, sample_rate_hz: f32) -> Self {
        let capacity = time_window.samples(sample_rate_hz) + 8;
        let filters = FilterChain::from_kinds(sample_rate_hz, &config.filters);
        Self {
            config,
            buffer: SampleBuffer::new(time_window.seconds, capacity),
            filters,
            last_sample_time: 0.0,
        }
    }
    fn ingest(&mut self, timestamp_secs: f32, value_uv: f32) {
        if !self.config.enabled {
            return;
        }
        let filtered = if self.filters.is_empty() {
            value_uv
        } else {
            self.filters.process_sample(value_uv)
        };
        self.last_sample_time = timestamp_secs;
        self.buffer.push(SamplePoint {
            time: timestamp_secs,
            value: filtered,
        });
    }
    fn view(&self) -> Option<ChannelView> {
        if !self.config.enabled || self.buffer.is_empty() {
            return None;
        }
        let mut samples: Vec<SamplePoint> = self.buffer.iter().copied().collect();
        if samples.is_empty() {
            return None;
        }
        let mut min = f32::MAX;
        let mut max = f32::MIN;
        let mut sum_sq: f32 = 0.0;
        for s in &samples {
            min = min.min(s.value);
            max = max.max(s.value);
            sum_sq += s.value * s.value;
        }
        let rms = (sum_sq / samples.len() as f32).sqrt();
        let y_range = match self.config.y_scale {
            YScale::Auto => {
                // Avoid a zero-height axis.
                let pad = ((max - min) * 0.1).max(1.0);
                (min - pad, max + pad)
            }
            YScale::FixedMicrovolts(mag) => (-mag, mag),
        };
        // Shift timestamps so callers can draw relative to the newest point if they want.
        let newest_time = samples.last().map(|s| s.time).unwrap_or(0.0);
        for s in &mut samples {
            s.time = s.time - newest_time;
        }
        Some(ChannelView {
            index: self.config.index,
            y_range,
            rms_u_v: rms,
            min,
            max,
            samples,
        })
    }
    fn set_time_window(&mut self, window: TimeWindow, sample_rate_hz: f32) {
        self.buffer.set_window(window.seconds.max(0.1));
        // Pre-allocate a bit of headroom to avoid churn.
        let desired_capacity = window.samples(sample_rate_hz) + 8;
        if self.buffer.len() > desired_capacity {
            // We already pruned older samples inside set_window.
        }
    }
    fn set_y_scale(&mut self, y_scale: YScale) {
        self.config.y_scale = y_scale;
    }
    fn set_filters(&mut self, sample_rate_hz: f32, filters: Vec<FilterKind>) {
        self.config.filters = filters;
        self.filters = FilterChain::from_kinds(sample_rate_hz, &self.config.filters);
    }
    fn set_enabled(&mut self, enabled: bool) {
        self.config.enabled = enabled;
    }
}
pub struct WaveformPipeline {
    sample_rate_hz: f32,
    time_window: TimeWindow,
    channels: Vec<ChannelState>,
    _started_at: Instant,
}
impl WaveformPipeline {
    pub fn new(channel_count: usize, sample_rate_hz: f32) -> Self {
        let time_window = TimeWindow::default();
        let channels = (0..channel_count)
            .map(|idx| ChannelState::new(ChannelConfig::new(idx), time_window, sample_rate_hz))
            .collect();
        Self {
            sample_rate_hz,
            time_window,
            channels,
            _started_at: Instant::now(),
        }
    }
    pub fn channel_count(&self) -> usize {
        self.channels.len()
    }
    pub fn set_time_window(&mut self, window: TimeWindow) {
        self.time_window = window;
        for channel in &mut self.channels {
            channel.set_time_window(window, self.sample_rate_hz);
        }
    }
    pub fn set_global_y_scale(&mut self, y_scale: YScale) {
        for channel in &mut self.channels {
            channel.set_y_scale(y_scale);
        }
    }
    pub fn set_channel_enabled(&mut self, index: usize, enabled: bool) {
        if let Some(ch) = self.channels.get_mut(index) {
            ch.set_enabled(enabled);
        }
    }
    pub fn set_channel_filters(&mut self, index: usize, filters: Vec<FilterKind>) {
        if let Some(ch) = self.channels.get_mut(index) {
            ch.set_filters(self.sample_rate_hz, filters);
        }
    }
    /// Ingest a single multi-channel frame. `timestamp_secs` should be monotonic.
    pub fn ingest_frame(&mut self, timestamp_secs: f32, microvolts_by_channel: &[f32]) {
        for (idx, value) in microvolts_by_channel.iter().enumerate() {
            if let Some(channel) = self.channels.get_mut(idx) {
                channel.ingest(timestamp_secs, *value);
            }
        }
    }
    /// Convenience for blocks of contiguous samples (shape: channels x samples).
    pub fn ingest_block(&mut self, start_time_secs: f32, samples_per_channel: &[Vec<f32>]) {
        let dt = 1.0 / self.sample_rate_hz;
        let max_samples = samples_per_channel
            .iter()
            .map(|ch| ch.len())
            .max()
            .unwrap_or(0);
        for i in 0..max_samples {
            let t = start_time_secs + i as f32 * dt;
            for (chan_idx, channel_samples) in samples_per_channel.iter().enumerate() {
                if let Some(val) = channel_samples.get(i) {
                    if let Some(channel) = self.channels.get_mut(chan_idx) {
                        channel.ingest(t, *val);
                    }
                }
            }
        }
    }
    pub fn view(&self) -> WaveformView {
        let mut channels = Vec::new();
        for channel in &self.channels {
            if let Some(view) = channel.view() {
                channels.push(view);
            }
        }
        WaveformView {
            window_secs: self.time_window.seconds,
            channels,
        }
    }
}
