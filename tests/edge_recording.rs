use std::fs;
use std::path::PathBuf;

use qnmd_sol::edge::recorder::{EdgeRecorder, RecordingMetadata};
use qnmd_sol::ssvep::SsvepDecision;

fn temp_dir() -> PathBuf {
    let unique = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_nanos();
    let dir = std::env::temp_dir()
        .join("neurostick-edge-recording")
        .join(format!("t-{unique}"));
    fs::create_dir_all(&dir).unwrap();
    dir
}

fn metadata_for_test() -> RecordingMetadata {
    RecordingMetadata {
        board_id: 2,
        serial_port: "/dev/openbci".to_string(),
        sample_rate_hz: 250.0,
        channels: vec![
            "Fp1", "Fp2", "C3", "C4", "P7", "P8", "O1", "O2", "F7", "F8", "F3", "F4", "T3", "T4",
            "P3", "P4",
        ]
        .into_iter()
        .map(|s| s.to_string())
        .collect(),
        target_freqs_hz: vec![8.0, 12.0, 15.0, 20.0],
        created_at_unix: 0,
    }
}

#[test]
fn writes_csv_and_ndjson_with_expected_headers() {
    let dir = temp_dir();
    let metadata = metadata_for_test();
    let mut recorder = EdgeRecorder::start(&dir, metadata).expect("start recorder");

    let sample = vec![
        0.1, 0.2, 0.3, 0.4, 0.5, 0.6, 0.7, 0.8, 0.9, 1.0, 1.1, 1.2, 1.3, 1.4, 1.5, 1.6,
    ];
    recorder.write_sample(0.123, &sample).unwrap();

    let decision = SsvepDecision {
        best_freq_hz: Some(12.0),
        scores: vec![(12.0, 0.8), (8.0, 0.2), (15.0, 0.1), (20.0, 0.05)],
        margin: 0.6,
        confident: true,
    };
    recorder.write_decision(0.5, &decision).unwrap();

    let paths = recorder.paths().clone();
    recorder.close().expect("close recorder");

    assert!(paths.samples_csv.exists(), "samples.csv missing");
    assert!(paths.decisions_ndjson.exists(), "decisions.ndjson missing");
    assert!(paths.metadata_json.exists(), "metadata.json missing");

    let csv = fs::read_to_string(&paths.samples_csv).unwrap();
    let mut lines = csv.lines();
    let header = lines.next().expect("csv header");
    assert!(
        header.starts_with("t_sec,Fp1,Fp2,"),
        "unexpected header: {header}"
    );
    assert!(header.ends_with(",P3,P4"), "header missing trailing channels: {header}");
    let row = lines.next().expect("csv row");
    assert!(row.starts_with("0.123"), "row should start with t_sec: {row}");

    let ndjson = fs::read_to_string(&paths.decisions_ndjson).unwrap();
    let line = ndjson.lines().next().expect("ndjson line");
    let parsed: serde_json::Value = serde_json::from_str(line).expect("decision json");
    assert_eq!(parsed["best_freq_hz"], 12.0);
    assert_eq!(parsed["confident"], true);

    let metadata_text = fs::read_to_string(&paths.metadata_json).unwrap();
    let metadata_json: serde_json::Value = serde_json::from_str(&metadata_text).unwrap();
    assert_eq!(metadata_json["board_id"], 2);
    assert_eq!(metadata_json["sample_rate_hz"], 250.0);
    assert!(metadata_json["channels"].as_array().unwrap().len() == 16);
}
