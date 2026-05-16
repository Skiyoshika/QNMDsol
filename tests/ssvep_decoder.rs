use qnmd_sol::ssvep::{SsvepConfig, SsvepDecoder};

fn sine(freq_hz: f32, sample_rate_hz: f32, seconds: f32) -> Vec<f32> {
    let n = (sample_rate_hz * seconds).round() as usize;
    (0..n)
        .map(|i| {
            let t = i as f32 / sample_rate_hz;
            (2.0 * std::f32::consts::PI * freq_hz * t).sin()
        })
        .collect()
}

#[test]
fn detects_12hz_target_from_clean_channels() {
    let cfg = SsvepConfig {
        target_freqs_hz: vec![8.0, 12.0, 15.0, 20.0],
        sample_rate_hz: 250.0,
        window_seconds: 2.0,
        harmonics: 2,
    };
    let decoder = SsvepDecoder::new(cfg);
    let channels = vec![
        sine(12.0, 250.0, 2.0),
        sine(12.0, 250.0, 2.0),
        sine(12.0, 250.0, 2.0),
        sine(12.0, 250.0, 2.0),
    ];

    let decision = decoder.decide(&channels);

    assert_eq!(decision.best_freq_hz, Some(12.0));
    assert!(decision.confident);
    assert!(decision.margin > 0.05);
}

#[test]
fn returns_uncertain_when_channels_are_empty() {
    let decoder = SsvepDecoder::new(SsvepConfig::default());
    let decision = decoder.decide(&[]);

    assert_eq!(decision.best_freq_hz, None);
    assert!(!decision.confident);
}
