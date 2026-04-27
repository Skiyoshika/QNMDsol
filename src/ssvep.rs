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
        let _ = channels;
        SsvepDecision {
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
        }
    }
}
