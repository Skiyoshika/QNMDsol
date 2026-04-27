use clap::Parser;

#[derive(Clone, Debug, Parser)]
pub struct EdgeConfig {
    #[arg(long, env = "NEUROSTICK_HOST", default_value = "0.0.0.0")]
    pub host: String,

    #[arg(long, env = "NEUROSTICK_PORT", default_value_t = 8765)]
    pub port: u16,

    #[arg(long, env = "OPENBCI_SERIAL", default_value = "/dev/openbci")]
    pub serial_port: String,

    #[arg(long, env = "OPENBCI_BOARD_ID", default_value_t = 2)]
    pub board_id: i32,

    #[arg(long, env = "NEUROSTICK_DATA_DIR", default_value = "/data")]
    pub data_dir: String,

    #[arg(long, env = "SSVEP_TARGET_FREQS", default_value = "8,12,15,20")]
    pub target_freqs: String,

    #[arg(long, env = "SSVEP_WINDOW_SEC", default_value_t = 2.0)]
    pub window_sec: f32,

    #[arg(long, env = "NEUROSTICK_SIMULATE", default_value_t = false)]
    pub simulate: bool,
}

impl EdgeConfig {
    pub fn target_freqs_hz(&self) -> Vec<f32> {
        self.target_freqs
            .split(',')
            .filter_map(|part| part.trim().parse::<f32>().ok())
            .collect()
    }
}
