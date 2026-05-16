# Windows + QEMU arm64 Verification Results

> Status: **PASS** — `linux/arm64` Docker image builds end-to-end on Windows
> via Docker Desktop / WSL2 / QEMU and the headless edge service answers all
> documented HTTP endpoints from the simulated source. This is the QEMU-based
> verification that complements the still-pending real Raspberry Pi 5
> hardware acceptance (`HARDWARE_RESULTS.md`).

Date: 2026-04-27
Host: Windows 11 (build 26200.8246), AMD Ryzen AI 7 H 350 (x86_64), WSL2
Docker: Docker Desktop 29.4.1 (build 055a478) with `desktop-linux` buildx instance
Buildkit: v0.29.0
Builder image: `docker/dockerfile:1.7`

## Build

Command:

```bash
export PATH="/c/Program Files/Docker/Docker/resources/bin:$PATH"
docker buildx build --platform linux/arm64 \
  -f Neurostick-Pi-5/Dockerfile \
  -t neurostick-pi5-edge:local \
  --load --progress plain .
```

Result:

```
#28 RUN file /opt/brainflow/lib/libBoardController.so
/opt/brainflow/lib/libBoardController.so:
  ELF 64-bit LSB shared object, ARM aarch64, version 1 (GNU/Linux),
  dynamically linked, BuildID[sha1]=66db42c8c8e6aa1af44b0fe75f27bee0068a6d4a,
  not stripped

#29 exporting to image
#29 naming to docker.io/library/neurostick-pi5-edge:local
#29 DONE 2.1s
BUILD_EXIT=0
```

`docker image inspect`:

```text
arch=arm64 os=linux size=36217432   (image ~35 MB)
```

## Smoke Test

```bash
docker run -d --rm --name neurostick-pi5-arm64-smoke --platform linux/arm64 \
  -p 18780:8765 \
  -e NEUROSTICK_SIMULATE=true \
  -e NEUROSTICK_DATA_DIR=/data \
  neurostick-pi5-edge:local
```

| Endpoint | Response |
|---|---|
| `GET /health`   | `{"ok":true}` |
| `POST /connect` | `{"ok":true}` |
| `POST /start`   | `{"ok":true}` |
| `GET /status`   | `{"connected":true,"streaming":true,"simulating":true,"sample_rate_hz":250.0,"eeg_channels":16,"channel_labels":["Fp1","Fp2","C3","C4","P7","P8","O1","O2","F7","F8","F3","F4","T3","T4","P3","P4"],"last_error":null}` |
| `GET /decision` | `{"decision":{"best_freq_hz":12.0,"confident":true,"margin":0.4999996,"scores":[[12.0,0.5000003],[8.0,6.6e-7],[15.0,6.3e-7],[20.0,4.9e-7]]}}` |
| `POST /stop`    | `{"ok":true}` |

The `/decision` response is the strongest smoke-test signal: the worker
thread, `SsvepDecoder`, and FFT path all run end-to-end inside the arm64
container under QEMU and correctly identify the 12 Hz synthetic carrier with
margin 0.5.

## What This Does NOT Cover

This is QEMU emulation on x86_64 hardware. It validates:

- The `Dockerfile` builds a `linux/arm64` image without manual fixes.
- The runtime image ships `aarch64` BrainFlow native libraries.
- The `pi_edge` HTTP API matches the documented contract.
- The SSVEP decode path works on arm64 instruction set.

It does not validate:

- Real Raspberry Pi 5 hardware (CPU temp, sustained throughput, memory growth).
- USB serial passthrough of the OpenBCI dongle (`/dev/serial/by-id/...` → `/dev/openbci`).
- Actual Cyton+Daisy capture at the real `125 Hz` board rate.
- 30-minute stability soak.

Those remain TODO and are tracked in `HARDWARE_RESULTS.md`.

## Dockerfile Fixes Discovered During This Run

1. `--platform=$BUILDPLATFORM` was changed to `--platform=$TARGETPLATFORM` so
   that BrainFlow source compilation happens under arm64 emulation; otherwise
   the runtime image would ship x86_64 `.so` files.
2. The `rust-builder` stage now `apt install` s `pkg-config libudev-dev
   libusb-1.0-0-dev libbluetooth-dev libssl-dev clang`. `libudev-sys` is
   pulled transitively via `winit -> eframe` even when only the headless
   `pi_edge` binary is built; without `libudev.pc` on the system library path
   the cargo build fails at `build.rs:38` with "system library `libudev` ...
   was not found".
