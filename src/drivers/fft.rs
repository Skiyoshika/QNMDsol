use rustfft::{num_complex::Complex32, FftPlanner};
use crate::drivers::TimeSeriesFrame;
/// Magnitude spectrum for each channel.
#[derive(Clone, Debug)]
pub struct FrequencySpectrum {
    pub sample_rate_hz: f32,
    pub frequencies_hz: Vec<f32>,
    pub magnitudes: Vec<Vec<f32>>, // channel -> bins
    pub channel_labels: Vec<String>,
}
/// Helper that computes FFTs for a given window size.
pub struct SpectrumBuilder {
    fft_size: usize,
}
impl SpectrumBuilder {
    pub fn with_size(fft_size: usize) -> Self {
        Self { fft_size }
    }
    pub fn compute(&self, frame: &TimeSeriesFrame) -> FrequencySpectrum {
        let mut planner = FftPlanner::<f32>::new();
        let fft = planner.plan_fft_forward(self.fft_size);
        let mut frequencies = Vec::with_capacity(self.fft_size / 2);
        for k in 0..self.fft_size / 2 {
            let freq = k as f32 * (frame.sample_rate_hz / self.fft_size as f32);
            frequencies.push(freq);
        }
        let magnitudes: Vec<Vec<f32>> = frame
            .samples
            .iter()
            .map(|channel| {
                let mut buffer: Vec<Complex32> = channel
                    .iter()
                    .copied()
                    .take(self.fft_size)
                    .map(|v| Complex32::new(v, 0.0))
                    .collect();
                buffer.resize(self.fft_size, Complex32::ZERO);
                fft.process(&mut buffer);
                buffer
                    .iter()
                    .take(self.fft_size / 2)
                    .map(|c| c.norm() / self.fft_size as f32)
                    .collect()
            })
            .collect();
        FrequencySpectrum {
            sample_rate_hz: frame.sample_rate_hz,
            frequencies_hz: frequencies,
            magnitudes,
            channel_labels: frame.channel_labels.clone(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::drivers::buffer::TimeSeriesFrame;
    use std::f32::consts::PI;

    fn sine_frame(freq_hz: f32, sample_rate: f32, n_samples: usize) -> TimeSeriesFrame {
        let samples: Vec<f32> = (0..n_samples)
            .map(|i| (2.0 * PI * freq_hz * i as f32 / sample_rate).sin())
            .collect();
        TimeSeriesFrame {
            sample_rate_hz: sample_rate,
            channel_labels: vec!["Ch1".into()],
            samples: vec![samples],
        }
    }

    #[test]
    fn fft_detects_dominant_frequency() {
        let frame = sine_frame(10.0, 256.0, 256);
        let builder = SpectrumBuilder::with_size(256);
        let spectrum = builder.compute(&frame);
        assert_eq!(spectrum.magnitudes.len(), 1);
        assert_eq!(spectrum.frequencies_hz.len(), 128);
        let peak_bin = spectrum.magnitudes[0]
            .iter()
            .enumerate()
            .max_by(|a, b| a.1.partial_cmp(b.1).unwrap())
            .unwrap()
            .0;
        let peak_freq = spectrum.frequencies_hz[peak_bin];
        assert!((peak_freq - 10.0).abs() < 2.0, "peak at {peak_freq} Hz, expected ~10 Hz");
    }

    #[test]
    fn fft_output_shape_matches_half_size() {
        let frame = sine_frame(20.0, 500.0, 128);
        let builder = SpectrumBuilder::with_size(64);
        let spectrum = builder.compute(&frame);
        assert_eq!(spectrum.frequencies_hz.len(), 32);
        assert_eq!(spectrum.magnitudes[0].len(), 32);
    }

    #[test]
    fn fft_dc_component_near_zero_for_sine() {
        let frame = sine_frame(25.0, 250.0, 250);
        let builder = SpectrumBuilder::with_size(250);
        let spectrum = builder.compute(&frame);
        assert!(spectrum.magnitudes[0][0] < 0.01, "DC should be near zero for pure sine");
    }

    #[test]
    fn fft_multichannel() {
        let frame = TimeSeriesFrame {
            sample_rate_hz: 256.0,
            channel_labels: vec!["Ch1".into(), "Ch2".into(), "Ch3".into()],
            samples: vec![vec![0.0; 64]; 3],
        };
        let builder = SpectrumBuilder::with_size(64);
        let spectrum = builder.compute(&frame);
        assert_eq!(spectrum.magnitudes.len(), 3);
        assert_eq!(spectrum.channel_labels.len(), 3);
    }
}
