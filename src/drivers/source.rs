use std::collections::VecDeque;
use std::time::{Duration, SystemTime};
use crate::drivers::ModelizeError;
/// Single batch of multi-channel EEG/EMG samples.
#[derive(Clone, Debug)]
pub struct SignalBatch {
    pub started_at: SystemTime,
    pub sample_rate_hz: f32,
    pub samples: Vec<Vec<f32>>, // channels x samples
    pub channel_labels: Vec<String>,
}
impl SignalBatch {
    pub fn validate(&self) -> Result<(), ModelizeError> {
        if self.sample_rate_hz <= 0.0 {
            return Err(ModelizeError::InvalidSampleRate);
        }
        let channel_count = self.samples.len();
        if channel_count != self.channel_labels.len() {
            return Err(ModelizeError::ChannelMismatch {
                expected: self.channel_labels.len(),
                actual: channel_count,
            });
        }
        Ok(())
    }
    pub fn num_channels(&self) -> usize {
        self.samples.len()
    }
    pub fn samples_per_channel(&self) -> Option<usize> {
        self.samples.first().map(|c| c.len())
    }
    pub fn duration(&self) -> Option<Duration> {
        self.samples_per_channel()
            .map(|len| Duration::from_secs_f32(len as f32 / self.sample_rate_hz))
    }
}
/// Trait representing something that can yield signal batches on demand.
pub trait SignalSource {
    fn next_batch(&mut self) -> Result<Option<SignalBatch>, ModelizeError>;
}
/// In-memory source useful for tests and deterministic playback.
pub struct ManualSource {
    queue: VecDeque<SignalBatch>,
}
impl ManualSource {
    pub fn new(batches: impl IntoIterator<Item = SignalBatch>) -> Self {
        Self {
            queue: batches.into_iter().collect(),
        }
    }
}
impl SignalSource for ManualSource {
    fn next_batch(&mut self) -> Result<Option<SignalBatch>, ModelizeError> {
        Ok(self.queue.pop_front())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn dummy_batch(rate: f32, n: usize) -> SignalBatch {
        SignalBatch {
            started_at: SystemTime::now(),
            sample_rate_hz: rate,
            samples: vec![vec![0.0; n]],
            channel_labels: vec!["Ch1".into()],
        }
    }

    #[test]
    fn batch_validate_accepts_valid() {
        let b = dummy_batch(250.0, 100);
        assert!(b.validate().is_ok());
    }

    #[test]
    fn batch_validate_rejects_zero_rate() {
        let b = dummy_batch(0.0, 100);
        assert!(b.validate().is_err());
    }

    #[test]
    fn batch_validate_rejects_label_mismatch() {
        let b = SignalBatch {
            started_at: SystemTime::now(),
            sample_rate_hz: 250.0,
            samples: vec![vec![0.0; 10], vec![0.0; 10]],
            channel_labels: vec!["Ch1".into()],
        };
        assert!(b.validate().is_err());
    }

    #[test]
    fn batch_duration_correct() {
        let b = dummy_batch(100.0, 200);
        let dur = b.duration().unwrap();
        assert!((dur.as_secs_f32() - 2.0).abs() < 1e-5);
    }

    #[test]
    fn manual_source_drains_queue() {
        let mut src = ManualSource::new(vec![dummy_batch(250.0, 10), dummy_batch(250.0, 20)]);
        let b1 = src.next_batch().unwrap().unwrap();
        assert_eq!(b1.samples[0].len(), 10);
        let b2 = src.next_batch().unwrap().unwrap();
        assert_eq!(b2.samples[0].len(), 20);
        assert!(src.next_batch().unwrap().is_none());
    }
}
