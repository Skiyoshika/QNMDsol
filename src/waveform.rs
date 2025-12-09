// Adapter module to surface the waveform pipeline that was added under `waveform-rs/`.
// We reuse the original source files directly so the logic stays in sync.
#[path = "waveform-rs/src/lib.rs"]
mod waveform_rs;

pub use waveform_rs::*;
