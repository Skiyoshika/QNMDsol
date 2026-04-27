# Hardware Results

> Status: **PENDING** — Task 10 of `IMPLEMENTATION_PLAN.md` requires execution on
> a real Raspberry Pi 5 with the OpenBCI Cyton+Daisy USB dongle attached. The
> implementation tasks 1–9 are complete in this branch. Fill in the values
> below once the container has been built on a Pi 5 and the API has been
> exercised end-to-end against the dongle.

Date:
Pi model: Raspberry Pi 5 (8 GB)
OS:
Docker version:
OpenBCI device path:
Board id: 2

## Commands

```text
# 1. Preflight
cd Neurostick/Neurostick-Pi-5
./scripts/pi5-preflight.sh

# 2. Pin a stable serial path for the dongle
export OPENBCI_DEVICE="$(find /dev/serial/by-id -maxdepth 1 -type l | head -n 1)"
echo "$OPENBCI_DEVICE"

# 3. Build + run
./scripts/run-pi5.sh

# 4. API exercise (in another shell)
./scripts/smoke-test.sh
curl -fsS -X POST http://127.0.0.1:8765/connect
curl -fsS -X POST http://127.0.0.1:8765/start
sleep 5
curl -fsS http://127.0.0.1:8765/snapshot
curl -fsS http://127.0.0.1:8765/decision
curl -fsS -X POST http://127.0.0.1:8765/record/start
sleep 10
curl -fsS -X POST http://127.0.0.1:8765/record/stop
```

## Results

```text
<paste health / status / snapshot / decision summaries>
```

## Stability

Duration:
Container restarted: yes/no
Data files created: yes/no
Observed issues:

## Build Architecture Verification

Run inside the container after build:

```bash
docker exec neurostick-pi5-edge file /opt/brainflow/lib/libBoardController.so
```

Expected (must show `aarch64`, not `x86-64`):

```text
ELF 64-bit LSB shared object, ARM aarch64, ...
```

Result:
