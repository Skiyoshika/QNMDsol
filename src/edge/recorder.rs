// Pi edge recorder. Implementation lands in Task 8.

use std::path::PathBuf;

#[derive(Debug, Clone)]
pub struct RecordingPaths {
    pub session_dir: PathBuf,
    pub samples_csv: PathBuf,
    pub decisions_ndjson: PathBuf,
    pub metadata_json: PathBuf,
}

impl RecordingPaths {
    pub fn under(data_dir: &str, session_name: &str) -> Self {
        let session_dir = PathBuf::from(data_dir).join(session_name);
        Self {
            samples_csv: session_dir.join("samples.csv"),
            decisions_ndjson: session_dir.join("decisions.ndjson"),
            metadata_json: session_dir.join("metadata.json"),
            session_dir,
        }
    }
}
