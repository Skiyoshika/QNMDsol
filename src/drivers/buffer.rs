use std::collections::VecDeque;
use crate::drivers::ModelizeError;
use crate::drivers::SignalBatch;
/// Flattened view of the current time-domain buffer.
#[derive(Clone, Debug)]
pub struct TimeSeriesFrame {
    pub sample_rate_hz: f32,
    pub channel_labels: Vec<String>,
    pub samples: Vec<Vec<f32>>, // channels x samples
}
impl TimeSeriesFrame {
    pub fn duration_seconds(&self) -> f32 {
        self.samples
            .first()
            .map(|c| c.len() as f32 / self.sample_rate_hz)
            .unwrap_or(0.0)
    }
}
/// Rolling buffer that stores recent samples per channel.
pub struct SignalBuffer {
    per_channel: Vec<VecDeque<f32>>, // channel -> samples
    channel_labels: Vec<String>,
    sample_rate_hz: f32,
    capacity: usize,
}
impl SignalBuffer {
    pub fn with_history_seconds(
        channel_labels: Vec<String>,
        sample_rate_hz: f32,
        history_seconds: f32,
    ) -> Result<Self, ModelizeError> {
        if sample_rate_hz <= 0.0 {
            return Err(ModelizeError::InvalidSampleRate);
        }
        let capacity = (sample_rate_hz * history_seconds).ceil() as usize;
        let per_channel = channel_labels
            .iter()
            .map(|_| VecDeque::with_capacity(capacity))
            .collect();
        Ok(Self {
            per_channel,
            channel_labels,
            sample_rate_hz,
            capacity,
        })
    }
    pub fn sample_rate_hz(&self) -> f32 {
        self.sample_rate_hz
    }
    pub fn channel_labels(&self) -> &[String] {
        &self.channel_labels
    }
    pub fn push_batch(&mut self, batch: &SignalBatch) -> Result<(), ModelizeError> {
        batch.validate()?;
        if batch.sample_rate_hz != self.sample_rate_hz {
            return Err(ModelizeError::SampleRateMismatch {
                expected: self.sample_rate_hz,
                actual: batch.sample_rate_hz,
            });
        }
        if batch.num_channels() != self.per_channel.len() {
            return Err(ModelizeError::ChannelMismatch {
                expected: self.per_channel.len(),
                actual: batch.num_channels(),
            });
        }
        for (channel_queue, new_samples) in self.per_channel.iter_mut().zip(&batch.samples) {
            for &sample in new_samples {
                if channel_queue.len() == self.capacity {
                    channel_queue.pop_front();
                }
                channel_queue.push_back(sample);
            }
        }
        Ok(())
    }
    pub fn snapshot(&self, seconds: f32) -> TimeSeriesFrame {
        let take = (self.sample_rate_hz * seconds).ceil() as usize;
        let samples: Vec<Vec<f32>> = self
            .per_channel
            .iter()
            .map(|channel| channel.iter().rev().take(take).rev().cloned().collect())
            .collect();
        TimeSeriesFrame {
            sample_rate_hz: self.sample_rate_hz,
            channel_labels: self.channel_labels.clone(),
            samples,
        }
    }
    pub fn full_frame(&self) -> TimeSeriesFrame {
        self.snapshot(self.capacity as f32 / self.sample_rate_hz)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::drivers::source::SignalBatch;
    use std::time::SystemTime;

    fn make_batch(rate: f32, samples: Vec<Vec<f32>>, labels: Vec<String>) -> SignalBatch {
        SignalBatch {
            started_at: SystemTime::now(),
            sample_rate_hz: rate,
            samples,
            channel_labels: labels,
        }
    }

    #[test]
    fn buffer_rejects_zero_sample_rate() {
        let result = SignalBuffer::with_history_seconds(vec!["Ch1".into()], 0.0, 1.0);
        assert!(result.is_err());
    }

    #[test]
    fn buffer_rejects_negative_sample_rate() {
        let result = SignalBuffer::with_history_seconds(vec!["Ch1".into()], -100.0, 1.0);
        assert!(result.is_err());
    }

    #[test]
    fn buffer_stores_and_snapshots() {
        let mut buf =
            SignalBuffer::with_history_seconds(vec!["Ch1".into(), "Ch2".into()], 250.0, 2.0)
                .unwrap();
        let batch = make_batch(
            250.0,
            vec![vec![1.0; 100], vec![2.0; 100]],
            vec!["Ch1".into(), "Ch2".into()],
        );
        buf.push_batch(&batch).unwrap();
        let frame = buf.snapshot(1.0);
        assert_eq!(frame.samples.len(), 2);
        assert_eq!(frame.samples[0].len(), 100);
        assert!(frame.samples[0].iter().all(|&v| v == 1.0));
        assert!(frame.samples[1].iter().all(|&v| v == 2.0));
    }

    #[test]
    fn buffer_rolls_off_old_samples() {
        let mut buf =
            SignalBuffer::with_history_seconds(vec!["Ch1".into()], 10.0, 1.0).unwrap();
        // capacity = ceil(10 * 1) = 10
        let batch_a = make_batch(10.0, vec![vec![1.0; 10]], vec!["Ch1".into()]);
        buf.push_batch(&batch_a).unwrap();
        let batch_b = make_batch(10.0, vec![vec![2.0; 5]], vec!["Ch1".into()]);
        buf.push_batch(&batch_b).unwrap();
        let frame = buf.full_frame();
        assert_eq!(frame.samples[0].len(), 10);
        assert!(frame.samples[0][5..].iter().all(|&v| v == 2.0));
    }

    #[test]
    fn buffer_rejects_sample_rate_mismatch() {
        let mut buf =
            SignalBuffer::with_history_seconds(vec!["Ch1".into()], 250.0, 1.0).unwrap();
        let batch = make_batch(500.0, vec![vec![0.0; 10]], vec!["Ch1".into()]);
        assert!(buf.push_batch(&batch).is_err());
    }

    #[test]
    fn buffer_rejects_channel_count_mismatch() {
        let mut buf =
            SignalBuffer::with_history_seconds(vec!["Ch1".into()], 250.0, 1.0).unwrap();
        let batch = make_batch(
            250.0,
            vec![vec![0.0; 10], vec![0.0; 10]],
            vec!["Ch1".into(), "Ch2".into()],
        );
        assert!(buf.push_batch(&batch).is_err());
    }

    #[test]
    fn duration_seconds_correct() {
        let frame = TimeSeriesFrame {
            sample_rate_hz: 250.0,
            channel_labels: vec!["Ch1".into()],
            samples: vec![vec![0.0; 500]],
        };
        assert!((frame.duration_seconds() - 2.0).abs() < 1e-5);
    }
}
