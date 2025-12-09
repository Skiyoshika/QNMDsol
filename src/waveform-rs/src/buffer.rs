use std::collections::VecDeque;
use super::view::SamplePoint;
pub struct SampleBuffer {
    data: VecDeque<SamplePoint>,
    window_secs: f32,
}
impl SampleBuffer {
    pub fn new(window_secs: f32, capacity: usize) -> Self {
        Self {
            data: VecDeque::with_capacity(capacity),
            window_secs,
        }
    }
    pub fn set_window(&mut self, window_secs: f32) {
        self.window_secs = window_secs.max(0.1);
        if let Some(last) = self.data.back().copied() {
            self.prune(last.time);
        }
    }
    pub fn push(&mut self, sample: SamplePoint) {
        self.data.push_back(sample);
        self.prune(sample.time);
    }
    pub fn iter(&self) -> impl Iterator<Item = &SamplePoint> {
        self.data.iter()
    }
    pub fn len(&self) -> usize {
        self.data.len()
    }
    pub fn is_empty(&self) -> bool {
        self.data.is_empty()
    }
    fn prune(&mut self, newest_time: f32) {
        let threshold = newest_time - self.window_secs;
        while let Some(front) = self.data.front() {
            if front.time < threshold {
                self.data.pop_front();
            } else {
                break;
            }
        }
    }
}
