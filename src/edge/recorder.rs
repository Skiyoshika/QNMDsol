use std::fs::{self, File, OpenOptions};
use std::io::{BufWriter, Write};
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

use serde::Serialize;

use crate::ssvep::SsvepDecision;

#[derive(Debug, Clone)]
pub struct RecordingPaths {
    pub session_dir: PathBuf,
    pub samples_csv: PathBuf,
    pub decisions_ndjson: PathBuf,
    pub metadata_json: PathBuf,
}

impl RecordingPaths {
    pub fn under(data_dir: &Path, session_name: &str) -> Self {
        let session_dir = data_dir.join(session_name);
        Self {
            samples_csv: session_dir.join("samples.csv"),
            decisions_ndjson: session_dir.join("decisions.ndjson"),
            metadata_json: session_dir.join("metadata.json"),
            session_dir,
        }
    }
}

#[derive(Debug, Clone, Serialize)]
pub struct RecordingMetadata {
    pub board_id: i32,
    pub serial_port: String,
    pub sample_rate_hz: f32,
    pub channels: Vec<String>,
    pub target_freqs_hz: Vec<f32>,
    pub created_at_unix: u64,
}

pub struct EdgeRecorder {
    paths: RecordingPaths,
    samples: BufWriter<File>,
    decisions: BufWriter<File>,
    metadata: RecordingMetadata,
}

impl EdgeRecorder {
    pub fn start(data_dir: &Path, mut metadata: RecordingMetadata) -> std::io::Result<Self> {
        let unix_ts = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map(|d| d.as_secs())
            .unwrap_or(0);
        if metadata.created_at_unix == 0 {
            metadata.created_at_unix = unix_ts;
        }
        let session_name = format!("session_{unix_ts}");
        let paths = RecordingPaths::under(data_dir, &session_name);
        fs::create_dir_all(&paths.session_dir)?;

        let samples_file = OpenOptions::new()
            .create(true)
            .write(true)
            .truncate(true)
            .open(&paths.samples_csv)?;
        let mut samples = BufWriter::new(samples_file);
        let header_cols: Vec<&str> = std::iter::once("t_sec")
            .chain(metadata.channels.iter().map(|s| s.as_str()))
            .collect();
        writeln!(samples, "{}", header_cols.join(","))?;
        samples.flush()?;

        let decisions_file = OpenOptions::new()
            .create(true)
            .write(true)
            .truncate(true)
            .open(&paths.decisions_ndjson)?;
        let decisions = BufWriter::new(decisions_file);

        let metadata_file = OpenOptions::new()
            .create(true)
            .write(true)
            .truncate(true)
            .open(&paths.metadata_json)?;
        let mut metadata_writer = BufWriter::new(metadata_file);
        let metadata_json = serde_json::to_string_pretty(&metadata)
            .map_err(|e| std::io::Error::new(std::io::ErrorKind::Other, e))?;
        metadata_writer.write_all(metadata_json.as_bytes())?;
        metadata_writer.flush()?;

        Ok(Self {
            paths,
            samples,
            decisions,
            metadata,
        })
    }

    pub fn paths(&self) -> &RecordingPaths {
        &self.paths
    }

    pub fn metadata(&self) -> &RecordingMetadata {
        &self.metadata
    }

    pub fn write_sample(&mut self, t_sec: f32, channels: &[f32]) -> std::io::Result<()> {
        write!(self.samples, "{:.6}", t_sec)?;
        for v in channels {
            write!(self.samples, ",{:.6}", v)?;
        }
        writeln!(self.samples)?;
        Ok(())
    }

    pub fn write_decision(&mut self, t_sec: f32, decision: &SsvepDecision) -> std::io::Result<()> {
        let line = serde_json::json!({
            "t_sec": t_sec,
            "best_freq_hz": decision.best_freq_hz,
            "margin": decision.margin,
            "confident": decision.confident,
            "scores": decision.scores,
        });
        let serialized =
            serde_json::to_string(&line).map_err(|e| std::io::Error::new(std::io::ErrorKind::Other, e))?;
        writeln!(self.decisions, "{serialized}")?;
        Ok(())
    }

    pub fn close(mut self) -> std::io::Result<()> {
        self.samples.flush()?;
        self.decisions.flush()?;
        Ok(())
    }
}
