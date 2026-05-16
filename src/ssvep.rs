use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct SsvepConfig {
    pub target_freqs_hz: Vec<f32>,
    pub sample_rate_hz: f32,
    pub window_seconds: f32,
    pub harmonics: usize,
}

impl Default for SsvepConfig {
    fn default() -> Self {
        Self {
            target_freqs_hz: vec![8.0, 12.0, 15.0, 20.0],
            sample_rate_hz: 250.0,
            window_seconds: 2.0,
            harmonics: 2,
        }
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct SsvepDecision {
    pub best_freq_hz: Option<f32>,
    pub scores: Vec<(f32, f32)>,
    pub margin: f32,
    pub confident: bool,
}

pub struct SsvepDecoder {
    config: SsvepConfig,
}

impl SsvepDecoder {
    pub fn new(config: SsvepConfig) -> Self {
        Self { config }
    }

    pub fn config(&self) -> &SsvepConfig {
        &self.config
    }

    pub fn decide(&self, channels: &[Vec<f32>]) -> SsvepDecision {
        if channels.is_empty() || channels.iter().all(|c| c.is_empty()) {
            return SsvepDecision {
                best_freq_hz: None,
                scores: self
                    .config
                    .target_freqs_hz
                    .iter()
                    .copied()
                    .map(|f| (f, 0.0))
                    .collect(),
                margin: 0.0,
                confident: false,
            };
        }

        let n = channels.iter().map(|c| c.len()).min().unwrap_or(0);
        if n < 32 {
            return SsvepDecision {
                best_freq_hz: None,
                scores: self
                    .config
                    .target_freqs_hz
                    .iter()
                    .copied()
                    .map(|f| (f, 0.0))
                    .collect(),
                margin: 0.0,
                confident: false,
            };
        }

        let mut scores = Vec::with_capacity(self.config.target_freqs_hz.len());
        for &target in &self.config.target_freqs_hz {
            let mut target_power = 0.0f32;
            for channel in channels {
                let usable = &channel[channel.len() - n..];
                let mean = usable.iter().copied().sum::<f32>() / usable.len() as f32;
                for harmonic in 1..=self.config.harmonics.max(1) {
                    let freq = target * harmonic as f32;
                    let mut sin_acc = 0.0f32;
                    let mut cos_acc = 0.0f32;
                    for (i, sample) in usable.iter().copied().enumerate() {
                        let t = i as f32 / self.config.sample_rate_hz;
                        let centered = sample - mean;
                        let phase = 2.0 * std::f32::consts::PI * freq * t;
                        sin_acc += centered * phase.sin();
                        cos_acc += centered * phase.cos();
                    }
                    target_power += (sin_acc * sin_acc + cos_acc * cos_acc).sqrt() / n as f32;
                }
            }
            scores.push((target, target_power / channels.len().max(1) as f32));
        }

        scores.sort_by(|a, b| b.1.total_cmp(&a.1));
        let best = scores.first().copied();
        let second = scores.get(1).copied();
        let margin = match (best, second) {
            (Some(a), Some(b)) => a.1 - b.1,
            (Some(a), None) => a.1,
            _ => 0.0,
        };
        let confident = best.map(|(_, s)| s > 0.05).unwrap_or(false) && margin > 0.05;

        SsvepDecision {
            best_freq_hz: if confident { best.map(|(f, _)| f) } else { None },
            scores,
            margin,
            confident,
        }
    }
}
