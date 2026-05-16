# Reviewer Checklist

Reviewer: Miki/Codex

## Must Pass

- Existing Windows desktop tests still pass.
- Pi edge code is available as a separate binary and does not require GUI/vJoy.
- BrainFlow loader supports Windows `.dll` and Linux `.so`.
- Docker image does not package x86_64 Linux BrainFlow libraries for arm64.
- Serial device path is configurable.
- Default board id is `2` for Cyton+Daisy.
- API returns structured JSON and no panics on disconnected hardware.
- SSVEP decoder has synthetic-signal tests.
- Recording output includes timestamps, channels, sampling rate, and decision metadata.
- Hardware acceptance evidence is attached to the PR.

## Reject Conditions

- Rewrites the whole desktop application.
- Breaks vJoy/Steam mapping paths.
- Requires running the Pi container with `--privileged` before trying `--device`.
- Stores large model files in the Docker image by default.
- Treats QEMU build success as hardware success.
