# Neurostick

Brain-Computer Interface (BCI) desktop app: reads EEG from OpenBCI hardware (Cyton/Ganglion), processes signals in real-time, and maps neural intent to virtual gamepad (vJoy) for hands-free gaming.

## Quick Start

```bash
cargo build          # Debug build
cargo run            # Launch GUI (simulation mode, no hardware needed)
cargo test           # Run unit tests
cargo clippy         # Lint
```

## Architecture

```
src/
  main.rs           # Entry point, eframe window setup, CJK font loading
  engine.rs         # Core BCI loop: acquire -> filter -> decode -> vJoy output
  gui.rs            # egui GUI (QnmdSolApp): waveform display, controls, model loading
  types.rs          # Shared enums: GuiCommand, BciMessage, GamepadState, SimInputIntent
  openbci.rs        # Serial port driver for OpenBCI boards
  vjoy.rs           # vJoy virtual joystick FFI via libloading
  recorder.rs       # CSV data recorder for training data collection
  visualizer.rs     # Visualization helpers
  waveform.rs       # Real-time waveform rendering pipeline
  brain_utils.rs    # ML model inference (CSP + LDA)
  assets.rs         # Embedded assets (icon PNG)
  drivers/
    mod.rs          # Public re-exports
    buffer.rs       # Rolling SignalBuffer with TimeSeriesFrame snapshots
    fft.rs          # FFT via rustfft -> FrequencySpectrum
    pipeline.rs     # SignalPipeline: source -> buffer -> spectrum
    source.rs       # SignalSource trait + ManualSource (test helper)
    plot.rs         # Plotters-based PNG rendering
    error.rs        # ModelizeError (thiserror)
    resistance_detection.rs  # Cyton/Ganglion impedance math

trainer/            # Python ML pipeline (scikit-learn, MNE)
  collect_data.py   # Record labeled EEG trials
  train_model.py    # Train CSP+LDA classifier
  export.py         # Export model to JSON for Rust
```

## Key Concepts

- **Engine thread** (`engine::spawn_thread`): runs in a dedicated thread, communicates with GUI via `mpsc` channels (`GuiCommand` -> engine, `BciMessage` -> GUI)
- **DSP pipeline**: 3 Hz highpass (drift removal) -> 50 Hz notch (powerline) -> threshold-based neural decoding
- **Two modes**: `ConnectionMode::Simulation` (keyboard-driven fake EEG) and `ConnectionMode::Hardware` (real OpenBCI via serial)
- **vJoy mapping**: neural intent patterns -> virtual gamepad buttons/axes for Steam Input

## Platform

- Windows only (winapi, vJoy driver)
- Requires vJoy driver installed for gamepad output
- OpenBCI Cyton/Ganglion via USB serial

## Conventions

- Comments in Chinese (Mandarin) are normal and expected
- Channel indices are 0-based, labels are 1-based ("Ch1" = index 0)
- Hardware data arrives in volts, UI displays in microvolts (1e6 scaling in engine.rs)
- Raw data is recorded to CSV; filtered data is sent to UI
