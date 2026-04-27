# Neurostick Pi 5 Edge Runtime

This folder contains the Raspberry Pi 5 deployment assets for running Neurostick acquisition and edge computation in a `linux/arm64` Docker container.

Docker is used as a deployable runtime, not as a full Raspberry Pi virtual machine. The final hardware test must run on a real Raspberry Pi 5 with the OpenBCI Cyton+Daisy USB dongle connected.

## Target Device

- Raspberry Pi 5
- 64-bit Raspberry Pi OS or Debian Bookworm arm64
- Docker Engine with BuildKit
- OpenBCI Cyton+Daisy
- OpenBCI USB dongle exposed as `/dev/serial/by-id/...`

## Runtime Services

- `pi_edge`: headless Rust service for acquisition, rolling buffer, FFT/SSVEP scoring, recording, and local API access.
- `Dockerfile`: arm64 image that builds or packages BrainFlow native libraries.
- `docker-compose.pi5.yml`: production run configuration for Pi 5.

## First Hardware Command

```bash
./scripts/pi5-preflight.sh
```

## Expected Container Command

```bash
docker compose -f docker-compose.pi5.yml up --build
```
