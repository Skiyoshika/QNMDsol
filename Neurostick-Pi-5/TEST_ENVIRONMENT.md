# Neurostick Pi 5 Test Environment

This is the test environment entrypoint for Windows host validation, Docker arm64 validation, and final Raspberry Pi 5 hardware acceptance.

## Layers

1. Windows host smoke: Rust build, unit/integration tests, simulated edge service, and OpenBCI COM port recording.
2. Docker arm64 smoke: build `linux/arm64` image with buildx and run simulated edge service under Docker.
3. Raspberry Pi 5 acceptance: run the container on Pi 5 with the OpenBCI Cyton+Daisy dongle mapped to `/dev/openbci`.

## Current Windows Host

Run from the repository root:

```powershell
powershell -ExecutionPolicy Bypass -File .\Neurostick-Pi-5\scripts\windows-smoke-test.ps1 -SerialPort COM3
```

Fast hardware-only iteration after a build:

```powershell
powershell -ExecutionPolicy Bypass -File .\Neurostick-Pi-5\scripts\windows-smoke-test.ps1 -SerialPort COM3 -SkipSlow
```

The script writes a JSON report under `target\test-env\<timestamp>\windows-smoke-report.json`.

Acceptance criteria:

- Simulation service starts, streams, and returns non-empty snapshots.
- Hardware service reports 16 EEG channels at 125 Hz for Cyton+Daisy.
- Recording rows stay within sampling-rate bounds, not CPU-loop speed.
- High saturation ratio is a signal-quality warning, not a transport failure.

## Windows Docker/WSL Setup

Check status:

```powershell
powershell -ExecutionPolicy Bypass -File .\Neurostick-Pi-5\scripts\windows-docker-preflight.ps1
```

Install WSL and Docker Desktop from an elevated PowerShell:

```powershell
powershell -ExecutionPolicy Bypass -File .\Neurostick-Pi-5\scripts\windows-docker-preflight.ps1 -InstallWSL -InstallDocker
```

Reboot if WSL or Docker asks for it, then open Docker Desktop once and enable the WSL 2 backend.

## Docker arm64 Verification

After Docker works:

```powershell
powershell -ExecutionPolicy Bypass -File .\Neurostick-Pi-5\scripts\docker-arm64-verify.ps1
```

This builds `neurostick-pi5-edge:local` for `linux/arm64`, checks the image architecture, runs it in simulation, and exercises the local HTTP API.

## Raspberry Pi 5 Acceptance

On the Pi 5:

```bash
cd Neurostick/Neurostick-Pi-5
./scripts/pi5-preflight.sh
export OPENBCI_DEVICE=/dev/serial/by-id/<openbci-dongle>
docker compose -f docker-compose.pi5.yml up --build
```

From another shell on the Pi:

```bash
BASE_URL=http://127.0.0.1:8765 ./scripts/smoke-test.sh
```

Final pass requires a real recording with plausible sample count and a separate signal-quality check for electrode saturation.
