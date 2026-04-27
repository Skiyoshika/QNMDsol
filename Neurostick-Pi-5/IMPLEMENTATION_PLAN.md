# Neurostick Pi 5 Edge Runtime Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Build a Docker-deployable Raspberry Pi 5 `linux/arm64` edge runtime that can acquire OpenBCI Cyton+Daisy EEG data, run lightweight SSVEP-oriented edge computation, expose local APIs, and preserve the existing Windows Neurostick desktop/game-control app.

**Architecture:** Do not rewrite Neurostick from scratch. Split reusable signal/acquisition code into a portable Rust library surface, add a headless Pi edge binary, and keep Windows-only GUI/vJoy code in the current desktop path. The Pi runtime uses BrainFlow native libraries compiled for `aarch64`, passes the OpenBCI USB serial device into Docker, and stores recordings/results on mounted host volumes.

**Tech Stack:** Rust 2021, BrainFlow C/C++ core, Docker BuildKit/buildx, Raspberry Pi OS 64-bit or Debian Bookworm arm64, OpenBCI Cyton+Daisy USB dongle, `rustfft`, `serde_json`, `tiny_http` or equivalent small HTTP server, optional Python tools only for offline validation/training.

---

## Operating Clarification

Docker is not a full Raspberry Pi virtual machine. It does not virtualize a Pi kernel or USB controller. The required deliverable is a `linux/arm64` container image that can run on a real Raspberry Pi 5 and can also be build-tested on an x86 workstation through QEMU/buildx. Hardware acceptance must happen on the real Pi 5 with the OpenBCI dongle attached.

## Repository Context

Current repo root: `D:\Neurostick`

Current useful assets:

- `src/openbci.rs`: BrainFlow C API wrapper, currently Windows-oriented because it loads `BoardController.dll`.
- `src/drivers/buffer.rs`: rolling signal buffer.
- `src/drivers/fft.rs`: FFT spectrum computation using `rustfft`.
- `src/drivers/pipeline.rs`: batch-to-buffer pipeline.
- `src/drivers/resistance_detection.rs`: Cyton impedance math.
- `src/model/neurogpt.rs`: ONNX runtime path, adaptive gate, channel labels.
- `src/recorder.rs`: CSV recording path, currently minimal.
- `src/gui.rs`, `src/vjoy.rs`, `src/main.rs`: Windows desktop/game-control surface; keep out of the Pi runtime.

Current known constraints:

- The root repo currently has uncommitted changes. Claude must inspect `git status --short --branch` before editing and must not revert unrelated user changes.
- The existing BrainFlow `.so` files in the repo root are not proven to be `linux/arm64`; treat them as desktop artifacts until verified with `file libBoardController.so`.
- BrainFlow PyPI does not provide ready Linux arm64 native libs for Raspberry Pi. The Docker image must compile BrainFlow core for `aarch64` or copy in a verified `aarch64` build artifact.
- vJoy is Windows-only. Pi output should be HTTP/WebSocket/NDJSON or UDP/TCP events, not vJoy.

## Target Runtime Behavior

The Pi service must support:

- Start in Docker on Raspberry Pi 5.
- Open `/dev/openbci`, mapped from a stable host serial device path such as `/dev/serial/by-id/...`.
- Connect to OpenBCI Cyton+Daisy with BrainFlow board id `2`.
- Stream 16 EEG channels at the board-reported sampling rate.
- Maintain a rolling window of recent samples.
- Compute:
  - basic signal stats,
  - FFT spectrum summary,
  - SSVEP target scores for default frequencies `8,12,15,20`,
  - current decision with margin/confidence.
- Record raw samples and event/decision records to mounted storage.
- Expose a small local API:
  - `GET /health`
  - `GET /status`
  - `POST /connect`
  - `POST /start`
  - `POST /stop`
  - `GET /snapshot`
  - `GET /decision`
  - `POST /record/start`
  - `POST /record/stop`
- Run without a GUI.

## Proposed File Structure

Create or modify these files during implementation:

```text
D:\Neurostick\
  Cargo.toml
  src\
    lib.rs
    openbci.rs
    ssvep.rs
    edge\
      mod.rs
      config.rs
      service.rs
      api.rs
      recorder.rs
    bin\
      pi_edge.rs
  tests\
    ssvep_decoder.rs
    edge_config.rs
  Neurostick-Pi-5\
    IMPLEMENTATION_PLAN.md
    README.md
    Dockerfile
    docker-compose.pi5.yml
    scripts\
      pi5-preflight.sh
      run-pi5.sh
      build-arm64.sh
      smoke-test.sh
    docs\
      REVIEW_CHECKLIST.md
      HARDWARE_ACCEPTANCE.md
      TROUBLESHOOTING.md
```

Keep existing Windows files in place:

```text
D:\Neurostick\src\main.rs
D:\Neurostick\src\gui.rs
D:\Neurostick\src\vjoy.rs
```

Do not move large model files into Docker by default:

```text
D:\Neurostick\model\neurogpt.onnx
D:\Neurostick\model\pytorch_model.bin
```

If future Pi inference needs ONNX, mount model files as a volume instead of copying them into the image.

---

## Task 1: Establish The Branch And Baseline

**Files:**
- Read: `D:\Neurostick\Cargo.toml`
- Read: `D:\Neurostick\src\openbci.rs`
- Read: `D:\Neurostick\src\drivers\*.rs`
- Read: `D:\Neurostick\src\model\neurogpt.rs`
- Read: `D:\Neurostick\README.md`

- [ ] **Step 1: Inspect current branch and dirty state**

Run:

```powershell
git status --short --branch
git diff --stat
cargo test --quiet
python -m unittest trainer\test_data_contract.py
```

Expected:

```text
cargo test: all existing Rust tests pass
python unittest: all existing Python data contract tests pass
```

If tests fail before implementation, save the exact output in `Neurostick-Pi-5/docs/BASELINE_FAILURES.md` and stop for reviewer review.

- [ ] **Step 2: Create an implementation branch**

Run:

```powershell
git switch -c miki/neurostick-pi5-edge
```

Expected:

```text
Switched to a new branch 'miki/neurostick-pi5-edge'
```

If the branch exists:

```powershell
git switch miki/neurostick-pi5-edge
```

- [ ] **Step 3: Record the baseline**

Create `D:\Neurostick\Neurostick-Pi-5\docs\BASELINE.md` with:

```markdown
# Baseline

Date: 2026-04-27

Branch:

```text
<paste `git status --short --branch` output>
```

Existing tests:

```text
<paste cargo test and Python unittest summary>
```

Known dirty files before Pi 5 work:

```text
<paste `git status --short` output>
```
```

Commit:

```powershell
git add Neurostick-Pi-5\docs\BASELINE.md
git commit -m "docs: record pi5 edge baseline"
```

---

## Task 2: Add Documentation And Hardware Preflight Scripts

**Files:**
- Create: `D:\Neurostick\Neurostick-Pi-5\README.md`
- Create: `D:\Neurostick\Neurostick-Pi-5\docs\HARDWARE_ACCEPTANCE.md`
- Create: `D:\Neurostick\Neurostick-Pi-5\docs\TROUBLESHOOTING.md`
- Create: `D:\Neurostick\Neurostick-Pi-5\docs\REVIEW_CHECKLIST.md`
- Create: `D:\Neurostick\Neurostick-Pi-5\scripts\pi5-preflight.sh`

- [ ] **Step 1: Write `README.md`**

Content:

```markdown
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
```

- [ ] **Step 2: Write `pi5-preflight.sh`**

Content:

```bash
#!/usr/bin/env bash
set -euo pipefail

echo "== OS =="
uname -a
dpkg --print-architecture || true

echo "== Docker =="
docker version || {
  echo "Docker is not installed or not available to this user." >&2
  exit 1
}

echo "== Serial devices =="
ls -la /dev/ttyUSB* /dev/ttyACM* 2>/dev/null || true
ls -la /dev/serial/by-id 2>/dev/null || {
  echo "No /dev/serial/by-id entries found. Plug in the OpenBCI dongle and retry." >&2
  exit 1
}

echo "== User groups =="
id
if ! id -nG | tr ' ' '\n' | grep -qx dialout; then
  echo "Current user is not in dialout. Run: sudo usermod -aG dialout $USER, then log out/in." >&2
  exit 1
fi

echo "== Candidate OpenBCI device paths =="
find /dev/serial/by-id -maxdepth 1 -type l -print

echo "Preflight complete."
```

Run:

```bash
chmod +x Neurostick-Pi-5/scripts/pi5-preflight.sh
shellcheck Neurostick-Pi-5/scripts/pi5-preflight.sh || true
```

Expected:

```text
The script is executable and prints OS, Docker, serial devices, and dialout status.
```

- [ ] **Step 3: Write hardware acceptance doc**

Content:

```markdown
# Hardware Acceptance

## Required Real-Hardware Tests

1. Pi 5 boots 64-bit OS.
2. Docker runs without sudo for the target user or the service user is documented.
3. OpenBCI dongle appears under `/dev/serial/by-id`.
4. Container receives the dongle as `/dev/openbci`.
5. `GET /health` returns `{"ok":true}`.
6. `POST /connect` connects to board id `2`.
7. `POST /start` begins streaming.
8. `GET /snapshot` returns 16 channel arrays.
9. `GET /decision` returns target scores for `8,12,15,20`.
10. Thirty minutes of streaming produces no process crash and no unbounded memory growth.

## Evidence To Capture

- `docker compose ps`
- `docker logs neurostick-pi5-edge --tail 100`
- `curl http://127.0.0.1:8765/health`
- `curl http://127.0.0.1:8765/status`
- A 10 second recording file under `./data`
```

- [ ] **Step 4: Write troubleshooting doc**

Content:

```markdown
# Troubleshooting

## Serial Device Missing

Run:

```bash
lsusb
dmesg | tail -80
ls -la /dev/serial/by-id
```

Use `/dev/serial/by-id/...` instead of `/dev/ttyUSB0` when possible.

## Permission Denied On Serial

Run:

```bash
id
sudo usermod -aG dialout "$USER"
```

Log out and log back in before retrying.

## BrainFlow Library Wrong Architecture

Run inside the container:

```bash
file /opt/brainflow/lib/libBoardController.so
```

Expected:

```text
ELF 64-bit ... ARM aarch64
```

If it reports `x86-64`, the image copied desktop libraries and must be rebuilt on arm64 or through buildx/QEMU.

## Board Opens But No Samples

Check:

- Cyton board is powered.
- Dongle and board are paired.
- Only one process owns the serial device.
- Board id is `2` for Cyton+Daisy.
```

- [ ] **Step 5: Write reviewer checklist**

Content:

```markdown
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
```

Commit:

```powershell
git add Neurostick-Pi-5
git commit -m "docs: add pi5 edge deployment guide"
```

---

## Task 3: Create A Portable Library Surface

**Files:**
- Create: `D:\Neurostick\src\lib.rs`
- Modify: `D:\Neurostick\Cargo.toml`
- Do not modify yet: `D:\Neurostick\src\main.rs`, `D:\Neurostick\src\gui.rs`, `D:\Neurostick\src\vjoy.rs`

- [ ] **Step 1: Add `src/lib.rs`**

Content:

```rust
pub mod drivers;
pub mod model;
pub mod openbci;
pub mod recorder;
pub mod ssvep;
pub mod types;
```

- [ ] **Step 2: Add initial module file for test-driven SSVEP work**

Create `D:\Neurostick\src\ssvep.rs`:

```rust
use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct SsvepConfig {
    pub target_freqs_hz: Vec<f32>,
    pub sample_rate_hz: f32,
    pub window_seconds: f32,
    pub harmonics: usize,
}

impl Default for SsvepConfig {
    fn default() -> Self {
        Self {
            target_freqs_hz: vec![8.0, 12.0, 15.0, 20.0],
            sample_rate_hz: 250.0,
            window_seconds: 2.0,
            harmonics: 2,
        }
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct SsvepDecision {
    pub best_freq_hz: Option<f32>,
    pub scores: Vec<(f32, f32)>,
    pub margin: f32,
    pub confident: bool,
}

pub struct SsvepDecoder {
    config: SsvepConfig,
}

impl SsvepDecoder {
    pub fn new(config: SsvepConfig) -> Self {
        Self { config }
    }

    pub fn config(&self) -> &SsvepConfig {
        &self.config
    }

    pub fn decide(&self, channels: &[Vec<f32>]) -> SsvepDecision {
        let _ = channels;
        SsvepDecision {
            best_freq_hz: None,
            scores: self.config.target_freqs_hz.iter().copied().map(|f| (f, 0.0)).collect(),
            margin: 0.0,
            confident: false,
        }
    }
}
```

- [ ] **Step 3: Verify library compiles**

Run:

```powershell
cargo test --quiet
```

Expected:

```text
Existing tests still pass.
```

Commit:

```powershell
git add src\lib.rs src\ssvep.rs Cargo.toml
git commit -m "refactor: expose portable neurostick library"
```

---

## Task 4: Implement And Test SSVEP Edge Decoder

**Files:**
- Modify: `D:\Neurostick\src\ssvep.rs`
- Create: `D:\Neurostick\tests\ssvep_decoder.rs`

- [ ] **Step 1: Write failing synthetic-signal tests**

Create `D:\Neurostick\tests\ssvep_decoder.rs`:

```rust
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
```

Run:

```powershell
cargo test --test ssvep_decoder --quiet
```

Expected:

```text
detects_12hz_target_from_clean_channels fails until decoder scoring is implemented.
```

- [ ] **Step 2: Implement FFT harmonic scoring**

Replace `decide` in `src\ssvep.rs` with an FFT peak-score implementation:

```rust
pub fn decide(&self, channels: &[Vec<f32>]) -> SsvepDecision {
    if channels.is_empty() || channels.iter().all(|c| c.is_empty()) {
        return SsvepDecision {
            best_freq_hz: None,
            scores: self.config.target_freqs_hz.iter().copied().map(|f| (f, 0.0)).collect(),
            margin: 0.0,
            confident: false,
        };
    }

    let n = channels.iter().map(|c| c.len()).min().unwrap_or(0);
    if n < 32 {
        return SsvepDecision {
            best_freq_hz: None,
            scores: self.config.target_freqs_hz.iter().copied().map(|f| (f, 0.0)).collect(),
            margin: 0.0,
            confident: false,
        };
    }

    let mut scores = Vec::with_capacity(self.config.target_freqs_hz.len());
    for &target in &self.config.target_freqs_hz {
        let mut target_power = 0.0f32;
        for channel in channels {
            let usable = &channel[channel.len() - n..];
            let mean = usable.iter().copied().sum::<f32>() / usable.len() as f32;
            for harmonic in 1..=self.config.harmonics.max(1) {
                let freq = target * harmonic as f32;
                let mut sin_acc = 0.0f32;
                let mut cos_acc = 0.0f32;
                for (i, sample) in usable.iter().copied().enumerate() {
                    let t = i as f32 / self.config.sample_rate_hz;
                    let centered = sample - mean;
                    let phase = 2.0 * std::f32::consts::PI * freq * t;
                    sin_acc += centered * phase.sin();
                    cos_acc += centered * phase.cos();
                }
                target_power += (sin_acc * sin_acc + cos_acc * cos_acc).sqrt() / n as f32;
            }
        }
        scores.push((target, target_power / channels.len().max(1) as f32));
    }

    scores.sort_by(|a, b| b.1.total_cmp(&a.1));
    let best = scores.first().copied();
    let second = scores.get(1).copied();
    let margin = match (best, second) {
        (Some(a), Some(b)) => a.1 - b.1,
        (Some(a), None) => a.1,
        _ => 0.0,
    };
    let confident = best.map(|(_, s)| s > 0.05).unwrap_or(false) && margin > 0.05;

    SsvepDecision {
        best_freq_hz: if confident { best.map(|(f, _)| f) } else { None },
        scores,
        margin,
        confident,
    }
}
```

- [ ] **Step 3: Run decoder tests**

Run:

```powershell
cargo test --test ssvep_decoder --quiet
```

Expected:

```text
2 passed
```

Commit:

```powershell
git add src\ssvep.rs tests\ssvep_decoder.rs
git commit -m "feat: add ssvep edge decoder"
```

---

## Task 5: Make BrainFlow Loading Cross-Platform

**Files:**
- Modify: `D:\Neurostick\src\openbci.rs`
- Create: `D:\Neurostick\tests\edge_config.rs`

- [ ] **Step 1: Add explicit library path resolution**

In `src\openbci.rs`, add:

```rust
fn brainflow_library_candidates() -> Vec<String> {
    if let Ok(path) = std::env::var("BRAINFLOW_BOARD_CONTROLLER") {
        if !path.trim().is_empty() {
            return vec![path];
        }
    }

    let mut candidates = Vec::new();
    if cfg!(target_os = "windows") {
        candidates.push("BoardController.dll".to_owned());
    } else if cfg!(target_os = "macos") {
        candidates.push("libBoardController.dylib".to_owned());
    } else {
        candidates.push("/opt/brainflow/lib/libBoardController.so".to_owned());
        candidates.push("libBoardController.so".to_owned());
    }
    candidates
}
```

Replace the direct load:

```rust
let lib = unsafe { Library::new("BoardController.dll") }
    .context("BoardController.dll not found in working directory")?;
```

with:

```rust
let mut last_error = None;
let mut loaded = None;
for candidate in brainflow_library_candidates() {
    match unsafe { Library::new(&candidate) } {
        Ok(lib) => {
            loaded = Some(lib);
            break;
        }
        Err(err) => {
            last_error = Some(format!("{candidate}: {err}"));
        }
    }
}
let lib = loaded.ok_or_else(|| {
    anyhow!(
        "BrainFlow BoardController library not found. Set BRAINFLOW_BOARD_CONTROLLER. Last error: {}",
        last_error.unwrap_or_else(|| "no candidates tried".to_owned())
    )
})?;
```

- [ ] **Step 2: Make board id configurable**

Replace:

```rust
const BOARD_ID_CYTON_DAISY: c_int = 2;
```

with:

```rust
pub const BOARD_ID_CYTON: c_int = 0;
pub const BOARD_ID_CYTON_DAISY: c_int = 2;
```

Add a `board_id: c_int` field to `OpenBciSession`, set it in constructors, and replace all hardcoded `BOARD_ID_CYTON_DAISY` calls in session methods with `self.board_id`.

Keep:

```rust
pub fn connect(port_name: &str) -> Result<Self> {
    Self::connect_with_board_id(port_name, BOARD_ID_CYTON_DAISY)
}
```

Add:

```rust
pub fn connect_with_board_id(port_name: &str, board_id: c_int) -> Result<Self> {
    let api = BrainFlowApi::instance()?;
    let params = BrainFlowInputParams::for_serial(port_name);
    let json = serde_json::to_string(&params)?;
    let input_json =
        CString::new(json).context("failed to encode BrainFlow input params to C string")?;
    api.prepare(board_id, &input_json)?;
    let sample_rate_hz = api.sampling_rate(board_id)? as f32;
    let num_rows = api.num_rows(board_id)? as usize;
    let eeg_channels = api.eeg_channels(board_id, num_rows)?;
    Ok(Self {
        port_name: port_name.to_string(),
        board_id,
        api,
        input_json,
        eeg_channels,
        num_rows,
        sample_rate_hz,
        is_streaming: false,
        released: false,
    })
}
```

- [ ] **Step 3: Run tests**

Run:

```powershell
cargo test --quiet
```

Expected:

```text
All existing tests pass.
```

Commit:

```powershell
git add src\openbci.rs
git commit -m "feat: support cross-platform brainflow loading"
```

---

## Task 6: Add Headless Pi Edge Service

**Files:**
- Modify: `D:\Neurostick\Cargo.toml`
- Create: `D:\Neurostick\src\edge\mod.rs`
- Create: `D:\Neurostick\src\edge\config.rs`
- Create: `D:\Neurostick\src\edge\service.rs`
- Create: `D:\Neurostick\src\edge\api.rs`
- Create: `D:\Neurostick\src\edge\recorder.rs`
- Modify: `D:\Neurostick\src\lib.rs`
- Create: `D:\Neurostick\src\bin\pi_edge.rs`

- [ ] **Step 1: Add small HTTP dependency**

In `Cargo.toml`, add:

```toml
clap = { version = "4.5", features = ["derive", "env"] }
tiny_http = "0.12"
```

- [ ] **Step 2: Expose `edge` module**

Modify `src\lib.rs`:

```rust
pub mod drivers;
pub mod edge;
pub mod model;
pub mod openbci;
pub mod recorder;
pub mod ssvep;
pub mod types;
```

- [ ] **Step 3: Implement config**

Create `src\edge\config.rs`:

```rust
use clap::Parser;

#[derive(Clone, Debug, Parser)]
pub struct EdgeConfig {
    #[arg(long, env = "NEUROSTICK_HOST", default_value = "0.0.0.0")]
    pub host: String,

    #[arg(long, env = "NEUROSTICK_PORT", default_value_t = 8765)]
    pub port: u16,

    #[arg(long, env = "OPENBCI_SERIAL", default_value = "/dev/openbci")]
    pub serial_port: String,

    #[arg(long, env = "OPENBCI_BOARD_ID", default_value_t = 2)]
    pub board_id: i32,

    #[arg(long, env = "NEUROSTICK_DATA_DIR", default_value = "/data")]
    pub data_dir: String,

    #[arg(long, env = "SSVEP_TARGET_FREQS", default_value = "8,12,15,20")]
    pub target_freqs: String,

    #[arg(long, env = "SSVEP_WINDOW_SEC", default_value_t = 2.0)]
    pub window_sec: f32,
}

impl EdgeConfig {
    pub fn target_freqs_hz(&self) -> Vec<f32> {
        self.target_freqs
            .split(',')
            .filter_map(|part| part.trim().parse::<f32>().ok())
            .collect()
    }
}
```

- [ ] **Step 4: Implement service state and API**

Create `src\edge\mod.rs`:

```rust
pub mod api;
pub mod config;
pub mod recorder;
pub mod service;
```

Create `src\bin\pi_edge.rs`:

```rust
use clap::Parser;
use qnmd_sol::edge::config::EdgeConfig;
use qnmd_sol::edge::service::run_edge_service;

fn main() -> anyhow::Result<()> {
    env_logger::init();
    let config = EdgeConfig::parse();
    run_edge_service(config)
}
```

Implement `src\edge\service.rs` so the first pass supports `GET /health` and `GET /status` before hardware:

```rust
use std::sync::{Arc, Mutex};

use serde::Serialize;
use tiny_http::{Header, Response, Server};

use crate::edge::config::EdgeConfig;

#[derive(Debug, Default, Serialize)]
pub struct EdgeState {
    pub connected: bool,
    pub streaming: bool,
    pub last_error: Option<String>,
}

fn json_response<T: Serialize>(value: &T) -> Response<std::io::Cursor<Vec<u8>>> {
    let body = serde_json::to_vec(value).unwrap_or_else(|_| b"{\"ok\":false}".to_vec());
    let header = Header::from_bytes(&b"Content-Type"[..], &b"application/json"[..]).unwrap();
    Response::from_data(body).with_header(header)
}

pub fn run_edge_service(config: EdgeConfig) -> anyhow::Result<()> {
    let bind = format!("{}:{}", config.host, config.port);
    let server = Server::http(&bind)?;
    let state = Arc::new(Mutex::new(EdgeState::default()));
    println!("neurostick pi edge listening on http://{bind}");

    for request in server.incoming_requests() {
        let path = request.url().split('?').next().unwrap_or("/");
        match (request.method().as_str(), path) {
            ("GET", "/health") => {
                request.respond(json_response(&serde_json::json!({"ok": true})))?;
            }
            ("GET", "/status") => {
                let guard = state.lock().unwrap();
                request.respond(json_response(&*guard))?;
            }
            _ => {
                request.respond(
                    Response::from_string("{\"error\":\"not found\"}")
                        .with_status_code(404)
                        .with_header(Header::from_bytes(&b"Content-Type"[..], &b"application/json"[..]).unwrap()),
                )?;
            }
        }
    }
    Ok(())
}
```

- [ ] **Step 5: Build and smoke test locally**

Run:

```powershell
cargo run --bin pi_edge -- --host 127.0.0.1 --port 8765
```

In another shell:

```powershell
Invoke-RestMethod http://127.0.0.1:8765/health
Invoke-RestMethod http://127.0.0.1:8765/status
```

Expected:

```text
/health returns ok=true
/status returns connected=false and streaming=false
```

Commit:

```powershell
git add Cargo.toml Cargo.lock src\lib.rs src\edge src\bin\pi_edge.rs
git commit -m "feat: add pi edge service skeleton"
```

---

## Task 7: Wire OpenBCI Streaming Into The Edge Service

**Files:**
- Modify: `D:\Neurostick\src\edge\service.rs`
- Modify: `D:\Neurostick\src\edge\api.rs`
- Modify: `D:\Neurostick\src\edge\recorder.rs`

- [ ] **Step 1: Extend state**

Add fields:

```rust
pub struct EdgeState {
    pub connected: bool,
    pub streaming: bool,
    pub sample_rate_hz: Option<f32>,
    pub eeg_channels: usize,
    pub latest_decision: Option<crate::ssvep::SsvepDecision>,
    pub last_error: Option<String>,
}
```

- [ ] **Step 2: Add streaming worker**

Use a background thread that owns `OpenBciSession`, `SignalBuffer`, and `SsvepDecoder`.

Behavior:

- `POST /connect` creates `OpenBciSession::connect_with_board_id`.
- `POST /start` calls `start_stream`.
- Worker pulls `next_sample`, converts to microvolts consistently, pushes into `SignalBuffer`, runs `SsvepDecoder` on posterior channels `O1/O2/P3/P4` when enough samples exist, and updates `latest_decision`.
- `POST /stop` stops stream.

Minimum response bodies:

```json
{"ok":true}
{"ok":false,"error":"..."}
```

- [ ] **Step 3: Add no-hardware mode for CI**

Add env/config:

```text
NEUROSTICK_SIMULATE=true
```

When enabled, skip BrainFlow and feed synthetic 12 Hz samples into the buffer. This allows Docker and API tests without the headset.

- [ ] **Step 4: Test simulated service**

Run:

```powershell
$env:NEUROSTICK_SIMULATE='true'
cargo run --bin pi_edge -- --host 127.0.0.1 --port 8765
```

Expected:

```text
GET /health returns ok=true
POST /start returns ok=true
GET /decision eventually returns best_freq_hz near 12
```

Commit:

```powershell
git add src\edge
git commit -m "feat: stream and decode eeg in pi edge service"
```

---

## Task 8: Add Recording Format

**Files:**
- Create/Modify: `D:\Neurostick\src\edge\recorder.rs`
- Modify: `D:\Neurostick\src\edge\service.rs`
- Create: `D:\Neurostick\tests\edge_recording.rs`

- [ ] **Step 1: Define output files**

When recording starts, create:

```text
/data/session_<unix_ts>/
  samples.csv
  decisions.ndjson
  metadata.json
```

`metadata.json`:

```json
{
  "board_id": 2,
  "serial_port": "/dev/openbci",
  "sample_rate_hz": 250.0,
  "channels": ["Fp1","Fp2","C3","C4","P7","P8","O1","O2","F7","F8","F3","F4","T3","T4","P3","P4"],
  "target_freqs_hz": [8.0,12.0,15.0,20.0],
  "created_at_unix": 0
}
```

`samples.csv` header:

```csv
t_sec,Fp1,Fp2,C3,C4,P7,P8,O1,O2,F7,F8,F3,F4,T3,T4,P3,P4
```

`decisions.ndjson` line:

```json
{"t_sec":1.25,"best_freq_hz":12.0,"margin":0.16,"confident":true,"scores":[[12.0,0.8],[8.0,0.2]]}
```

- [ ] **Step 2: Add tests**

Test should create a temporary directory under `target/tmp-edge-recording`, start recorder, write one sample row and one decision row, close recorder, and assert files exist with expected headers.

Run:

```powershell
cargo test --test edge_recording --quiet
```

Expected:

```text
recording files are created and contain expected headers
```

Commit:

```powershell
git add src\edge\recorder.rs src\edge\service.rs tests\edge_recording.rs
git commit -m "feat: record pi edge samples and decisions"
```

---

## Task 9: Add Docker Build And Compose For Pi 5

**Files:**
- Create: `D:\Neurostick\Neurostick-Pi-5\Dockerfile`
- Create: `D:\Neurostick\Neurostick-Pi-5\docker-compose.pi5.yml`
- Create: `D:\Neurostick\Neurostick-Pi-5\scripts\build-arm64.sh`
- Create: `D:\Neurostick\Neurostick-Pi-5\scripts\run-pi5.sh`
- Create: `D:\Neurostick\Neurostick-Pi-5\scripts\smoke-test.sh`

- [ ] **Step 1: Dockerfile**

Use this baseline. Claude may optimize later, but must preserve arm64 BrainFlow verification.

```dockerfile
# syntax=docker/dockerfile:1.7

FROM --platform=$BUILDPLATFORM debian:bookworm AS brainflow-builder
ARG TARGETARCH
ARG BRAINFLOW_REF=master
RUN apt-get update && apt-get install -y --no-install-recommends \
    ca-certificates git python3 python3-pip cmake ninja-build build-essential pkg-config \
    libbluetooth-dev libusb-1.0-0-dev \
    && rm -rf /var/lib/apt/lists/*
RUN git clone --depth 1 https://github.com/brainflow-dev/brainflow.git /opt/brainflow-src
WORKDIR /opt/brainflow-src
RUN python3 tools/build.py
RUN mkdir -p /opt/brainflow/lib \
    && find /opt/brainflow-src -name 'libBoardController.so' -exec cp {} /opt/brainflow/lib/ \; \
    && find /opt/brainflow-src -name 'libDataHandler.so' -exec cp {} /opt/brainflow/lib/ \; \
    && find /opt/brainflow-src -name 'libBrainFlowBluetooth.so' -exec cp {} /opt/brainflow/lib/ \; \
    && test -f /opt/brainflow/lib/libBoardController.so \
    && file /opt/brainflow/lib/libBoardController.so

FROM --platform=$BUILDPLATFORM rust:1-bookworm AS rust-builder
WORKDIR /work
COPY Cargo.toml Cargo.lock ./
COPY src ./src
RUN cargo build --release --bin pi_edge

FROM debian:bookworm-slim AS runtime
RUN apt-get update && apt-get install -y --no-install-recommends \
    ca-certificates libstdc++6 libgcc-s1 libusb-1.0-0 libbluetooth3 curl file \
    && rm -rf /var/lib/apt/lists/*
COPY --from=brainflow-builder /opt/brainflow/lib /opt/brainflow/lib
COPY --from=rust-builder /work/target/release/pi_edge /usr/local/bin/pi_edge
ENV BRAINFLOW_BOARD_CONTROLLER=/opt/brainflow/lib/libBoardController.so
ENV NEUROSTICK_HOST=0.0.0.0
ENV NEUROSTICK_PORT=8765
ENV OPENBCI_SERIAL=/dev/openbci
ENV OPENBCI_BOARD_ID=2
ENV NEUROSTICK_DATA_DIR=/data
RUN file /opt/brainflow/lib/libBoardController.so
VOLUME ["/data"]
EXPOSE 8765
ENTRYPOINT ["/usr/local/bin/pi_edge"]
```

- [ ] **Step 2: Compose file**

Create `docker-compose.pi5.yml`:

```yaml
services:
  neurostick-pi5-edge:
    build:
      context: ..
      dockerfile: Neurostick-Pi-5/Dockerfile
    image: neurostick-pi5-edge:local
    container_name: neurostick-pi5-edge
    restart: unless-stopped
    ports:
      - "8765:8765"
    devices:
      - "${OPENBCI_DEVICE:-/dev/serial/by-id/replace-me}:/dev/openbci"
    environment:
      OPENBCI_SERIAL: /dev/openbci
      OPENBCI_BOARD_ID: "2"
      NEUROSTICK_DATA_DIR: /data
      SSVEP_TARGET_FREQS: "8,12,15,20"
      SSVEP_WINDOW_SEC: "2.0"
    volumes:
      - ./data:/data
```

- [ ] **Step 3: Build script**

Create `scripts/build-arm64.sh`:

```bash
#!/usr/bin/env bash
set -euo pipefail

cd "$(dirname "$0")/../.."
docker buildx build \
  --platform linux/arm64 \
  -f Neurostick-Pi-5/Dockerfile \
  -t neurostick-pi5-edge:local \
  --load \
  .
```

- [ ] **Step 4: Run script**

Create `scripts/run-pi5.sh`:

```bash
#!/usr/bin/env bash
set -euo pipefail

cd "$(dirname "$0")/.."
: "${OPENBCI_DEVICE:?Set OPENBCI_DEVICE=/dev/serial/by-id/<dongle>}"
docker compose -f docker-compose.pi5.yml up --build
```

- [ ] **Step 5: Smoke test script**

Create `scripts/smoke-test.sh`:

```bash
#!/usr/bin/env bash
set -euo pipefail

BASE_URL="${BASE_URL:-http://127.0.0.1:8765}"
curl -fsS "$BASE_URL/health"
echo
curl -fsS "$BASE_URL/status"
echo
```

Run:

```bash
chmod +x Neurostick-Pi-5/scripts/*.sh
```

Commit:

```powershell
git add Neurostick-Pi-5\Dockerfile Neurostick-Pi-5\docker-compose.pi5.yml Neurostick-Pi-5\scripts
git commit -m "build: add pi5 docker runtime"
```

---

## Task 10: Pi 5 Hardware Acceptance

**Files:**
- Modify: `D:\Neurostick\Neurostick-Pi-5\docs\HARDWARE_ACCEPTANCE.md`
- Create: `D:\Neurostick\Neurostick-Pi-5\docs\HARDWARE_RESULTS.md`

- [ ] **Step 1: Run preflight on Pi**

Run on Raspberry Pi 5:

```bash
cd Neurostick/Neurostick-Pi-5
./scripts/pi5-preflight.sh
```

Expected:

```text
Preflight complete.
```

- [ ] **Step 2: Set stable serial device**

Run:

```bash
export OPENBCI_DEVICE="$(find /dev/serial/by-id -maxdepth 1 -type l | head -n 1)"
echo "$OPENBCI_DEVICE"
```

Expected:

```text
/dev/serial/by-id/<actual OpenBCI dongle path>
```

- [ ] **Step 3: Run container**

Run:

```bash
./scripts/run-pi5.sh
```

Expected:

```text
Container builds and starts. Logs show pi_edge listening on http://0.0.0.0:8765.
```

- [ ] **Step 4: Verify API**

Run:

```bash
./scripts/smoke-test.sh
curl -fsS -X POST http://127.0.0.1:8765/connect
curl -fsS -X POST http://127.0.0.1:8765/start
sleep 5
curl -fsS http://127.0.0.1:8765/snapshot
curl -fsS http://127.0.0.1:8765/decision
```

Expected:

```text
/snapshot returns 16 channels or documented board-reported channel count.
/decision returns scores for 8, 12, 15, 20 Hz.
```

- [ ] **Step 5: Record evidence**

Create `docs/HARDWARE_RESULTS.md`:

```markdown
# Hardware Results

Date:
Pi model:
OS:
Docker version:
OpenBCI device path:
Board id:

## Commands

```text
<paste commands run>
```

## Results

```text
<paste health/status/snapshot/decision summaries>
```

## Stability

Duration:
Container restarted: yes/no
Data files created: yes/no
Observed issues:
```
```

Commit:

```bash
git add Neurostick-Pi-5/docs/HARDWARE_RESULTS.md
git commit -m "test: record pi5 hardware acceptance"
```

---

## Reviewer Handoff

Before asking Codex/Miki for review, Claude must provide:

- `git status --short --branch`
- `git log --oneline -10`
- `cargo test --quiet` result
- `cargo test --test ssvep_decoder --quiet` result
- `python -m unittest trainer\test_data_contract.py` result
- Docker build output showing `libBoardController.so` architecture
- Pi hardware evidence from `Neurostick-Pi-5/docs/HARDWARE_RESULTS.md`

Review request should say:

```text
Please review the Neurostick Pi 5 edge runtime implementation. Focus on:
1. Whether Windows desktop/vJoy behavior was preserved.
2. Whether BrainFlow native libraries are correctly handled for linux/arm64.
3. Whether the Pi service is safe around serial device ownership and reconnects.
4. Whether SSVEP decoding tests prove the edge compute path.
5. Whether Docker/compose can run on Pi 5 without privileged mode.
```

## Final Acceptance Criteria

- Windows desktop path still builds/tests on the existing development machine.
- `pi_edge` builds as a headless binary.
- The Docker image builds for `linux/arm64`.
- The Docker runtime uses `/dev/openbci`, not Windows `COM*`.
- BrainFlow native library in the runtime image is `aarch64`.
- The service handles disconnected hardware with JSON errors, not panics.
- Synthetic SSVEP test identifies 12 Hz.
- Real Pi 5 can stream OpenBCI Cyton+Daisy data.
- Recording output is written to `Neurostick-Pi-5/data` or mounted `/data`.
- No large ONNX/PyTorch model files are copied into the image by default.
