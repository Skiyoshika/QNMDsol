# Waveform Pipeline (Rust)

This crate extracts the Time Series rendering logic from the OpenBCI GUI and rewrites the data-side pieces in Rust so it can be dropped into any application. It keeps the responsibilities that make the waveform widget behave correctly (windowing, scaling, filtering, and per-channel state) but leaves the actual drawing to your host app.

## Key features mapped from the GUI
- Time window control (1–20 s) with resizable buffers.
- Vertical scaling: fixed ±µV ranges or autoscale based on visible data.
- Per-channel enable/disable and filter chains (notch, band-pass, band-stop, low/high-pass).
- Windowed stats (min, max, RMS) mirroring the values shown next to each channel in the GUI.
- Multi-channel ingestion helpers for streaming and playback blocks.

## Modules
- `config.rs` – time window and Y-scale types.
- `filter.rs` – small biquad filter toolkit and `FilterChain`.
- `buffer.rs` – ring buffer that trims to the current window.
- `channel.rs` – per-channel state and the `WaveformPipeline` orchestrator.
- `view.rs` – lightweight structs returned to your renderer (`WaveformView`, `ChannelView`, `SamplePoint`).

## Quick start
```rust
use waveform_pipeline::{FilterKind, TimeWindow, WaveformPipeline, YScale};

// 8 channels @ 250 Hz
let mut pipe = WaveformPipeline::new(8, 250.0);
pipe.set_time_window(TimeWindow::new(5.0));
pipe.set_global_y_scale(YScale::FixedMicrovolts(200.0));

// Optional filters: 60 Hz notch + 1-50 Hz bandpass
pipe.set_channel_filters(0, vec![
    FilterKind::Notch { freq_hz: 60.0, q: 35.0 },
    FilterKind::Bandpass { low_hz: 1.0, high_hz: 50.0, q: 0.707 },
]);

// Feed one frame of microvolt data
pipe.ingest_frame(12.345, &[0.4, -1.2, 0.0, 1.3, 0.8, 0.1, -0.6, 0.2]);

// Grab data to draw
let view = pipe.view();
for chan in view.channels {
    println!("ch {} rms {:.2}uV range {:?}", chan.index + 1, chan.rms_u_v, chan.y_range);
}
```

## egui/eframe demo
- Run the included demo with `cargo run --example egui_viewer --release` to see the pipeline embedded in an egui/eframe window on Windows/macOS/Linux.
- The example streams synthetic data into `WaveformPipeline`, then draws each channel with `egui::plot::Plot` using the relative timestamps returned by `WaveformView` (x axis goes from `-window_secs` to `0`).
