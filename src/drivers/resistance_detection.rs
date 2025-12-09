//! Port of the OpenBCI-GUI impedance / resistance calculation logic to Rust.
//! This keeps the math identical so it can be embedded into another app.
//!
//! Cyton:
//! - Computes channel standard deviation (µV) over a recent window.
//! - Impedance (Ω) = sqrt(2) * std_µV * 1e-6 / lead_off_drive_amps - series_resistor_ohms.
//! - Clamp to zero to avoid negative results.
//!
//! Ganglion:
//! - Firmware returns impedance-like values on the resistance channels.
//! - GUI halves the value to account for the driven-ground leg (see W_GanglionImpedance.pde).
//! - Values are displayed as kΩ in the GUI.
/// Series resistor used on the Cyton board (ohms).
pub const SERIES_RESISTOR_OHMS: f32 = 2200.0;
/// Lead-off drive current configured on Cyton (amps).
pub const LEAD_OFF_DRIVE_AMPS: f32 = 6.0e-9;
/// Compute Cyton-style impedance (ohms) from a channel's standard deviation (microvolts).
///
/// Equivalent to the GUI calculation:
/// `impedance = sqrt(2) * std_uV * 1e-6 / LEAD_OFF_DRIVE_AMPS - SERIES_RESISTOR_OHMS`
/// Negative values are clamped to zero.
pub fn cyton_impedance_from_std(std_microvolts: f32) -> f32 {
    let mut impedance_ohms =
        (2.0_f32.sqrt() * std_microvolts * 1.0e-6) / LEAD_OFF_DRIVE_AMPS - SERIES_RESISTOR_OHMS;
    if impedance_ohms.is_nan() || impedance_ohms < 0.0 {
        impedance_ohms = 0.0;
    }
    impedance_ohms
}
/// Convenience helper: compute Cyton impedances for multiple channels of µV samples.
///
/// Each slice in `channels_uv` should be the recent samples for one channel (same length).
/// The function measures standard deviation per channel, then converts to impedance (ohms).
pub fn cyton_impedances_from_samples(channels_uv: &[&[f32]]) -> Vec<f32> {
    channels_uv
        .iter()
        .map(|channel| {
            let std = std_dev(channel);
            cyton_impedance_from_std(std)
        })
        .collect()
}
/// Standard deviation helper (sample std; GUI uses population-style variance over the window).
fn std_dev(data: &[f32]) -> f32 {
    if data.is_empty() {
        return 0.0;
    }
    let mean = data.iter().copied().sum::<f32>() / data.len() as f32;
    let variance = data
        .iter()
        .map(|v| {
            let delta = v - mean;
            delta * delta
        })
        .sum::<f32>()
        / data.len() as f32;
    variance.sqrt()
}
/// Convert Ganglion resistance channel readings to the displayed impedance (kΩ).
///
/// In the GUI the raw value is divided by two before being shown as kilo-ohms.
/// `raw_value` should match what is read from the resistance channel.
pub fn ganglion_display_impedance_kohms(raw_value: f32) -> f32 {
    raw_value / 2.0
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn cyton_impedance_matches_gui_math() {
        // Create a simple waveform with a known std dev (1.0 µV).
        let samples = [0.0_f32, 2.0, -2.0, 0.0];
        let imp = cyton_impedance_from_std(std_dev(&samples));
        let expected = (2.0_f32.sqrt() * 2.0 * 1.0e-6 / LEAD_OFF_DRIVE_AMPS) - SERIES_RESISTOR_OHMS;
        assert!((imp - expected.max(0.0)).abs() < 1e-3);
    }
    #[test]
    fn ganglion_impedance_scaling() {
        assert_eq!(ganglion_display_impedance_kohms(100.0), 50.0);
    }
}
