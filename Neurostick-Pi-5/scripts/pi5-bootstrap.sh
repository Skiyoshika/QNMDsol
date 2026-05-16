#!/usr/bin/env bash
# One-shot Pi 5 bootstrap: install docker, clone the repo, build the arm64
# image, run sim mode, and print the smoke-test verdict.
#
# Usage on a fresh Raspberry Pi OS 64-bit:
#   curl -fsSL https://raw.githubusercontent.com/Skiyoshika/Neurostick/miki/neurostick-pi5-edge/Neurostick-Pi-5/scripts/pi5-bootstrap.sh | bash
#
# Or, if the repo is already cloned:
#   cd ~/Neurostick && ./Neurostick-Pi-5/scripts/pi5-bootstrap.sh

set -euo pipefail

REPO_URL="${REPO_URL:-https://github.com/Skiyoshika/Neurostick.git}"
REPO_BRANCH="${REPO_BRANCH:-miki/neurostick-pi5-edge}"
REPO_DIR="${REPO_DIR:-$HOME/Neurostick}"

step() { printf '\n\033[1;36m==== %s ====\033[0m\n' "$*"; }

step "1/6  System info"
uname -a
dpkg --print-architecture || true

if [ "$(dpkg --print-architecture 2>/dev/null || echo unknown)" != "arm64" ]; then
    echo "WARNING: this script expects arm64. Continuing anyway."
fi

step "2/6  Install Docker (idempotent)"
if command -v docker >/dev/null 2>&1; then
    echo "docker already installed: $(docker --version)"
else
    curl -fsSL https://get.docker.com | sh
    sudo usermod -aG docker "$USER"
    echo "NOTE: log out and back in (or 'newgrp docker') to use docker without sudo."
fi
DOCKER="docker"
if ! docker info >/dev/null 2>&1; then
    DOCKER="sudo docker"
    echo "Falling back to 'sudo docker' for the rest of this run."
fi

step "3/6  Clone or update repo"
if [ -d "$REPO_DIR/.git" ]; then
    cd "$REPO_DIR"
    git fetch origin "$REPO_BRANCH"
    git checkout "$REPO_BRANCH"
    git pull --ff-only origin "$REPO_BRANCH" || true
else
    git clone --branch "$REPO_BRANCH" "$REPO_URL" "$REPO_DIR"
    cd "$REPO_DIR"
fi
echo "Repo at: $(pwd) ($(git rev-parse --short HEAD))"

step "4/6  Build arm64 image (first build compiles BrainFlow from source, ~30-60 min)"
$DOCKER build \
    -f Neurostick-Pi-5/Dockerfile \
    -t neurostick-pi5-edge:local \
    .

step "5/6  Verify image architecture"
ARCH=$($DOCKER image inspect neurostick-pi5-edge:local --format '{{.Architecture}}')
echo "Image architecture: $ARCH"
if [ "$ARCH" != "arm64" ]; then
    echo "ERROR: expected arm64, got $ARCH" >&2
    exit 1
fi
$DOCKER run --rm neurostick-pi5-edge:local file /opt/brainflow/lib/libBoardController.so

step "6/6  Smoke test in simulation mode"
CONTAINER=neurostick-pi5-edge-smoke
$DOCKER rm -f $CONTAINER 2>/dev/null || true
$DOCKER run -d --name $CONTAINER \
    -p 8765:8765 \
    -e NEUROSTICK_SIMULATE=true \
    -e NEUROSTICK_DATA_DIR=/data \
    neurostick-pi5-edge:local
trap '$DOCKER rm -f $CONTAINER >/dev/null 2>&1 || true' EXIT

echo "Waiting for /health..."
for i in $(seq 1 30); do
    if curl -fsS http://127.0.0.1:8765/health >/dev/null 2>&1; then
        break
    fi
    sleep 1
done

echo
echo "GET /health    -> $(curl -fsS http://127.0.0.1:8765/health)"
curl -fsS -X POST http://127.0.0.1:8765/connect >/dev/null
curl -fsS -X POST http://127.0.0.1:8765/start >/dev/null
sleep 4
echo "GET /status    -> $(curl -fsS http://127.0.0.1:8765/status)"
echo "GET /decision  -> $(curl -fsS http://127.0.0.1:8765/decision)"
curl -fsS -X POST http://127.0.0.1:8765/stop >/dev/null

echo
printf '\033[1;32mSMOKE PASSED.\033[0m  Pi 5 can build and run the arm64 image.\n'
echo "Next:"
echo "  1. Plug in OpenBCI Cyton+Daisy dongle."
echo "  2. ./Neurostick-Pi-5/scripts/pi5-preflight.sh"
echo "  3. export OPENBCI_DEVICE=\$(find /dev/serial/by-id -maxdepth 1 -type l | head -n 1)"
echo "  4. cd Neurostick-Pi-5 && docker compose -f docker-compose.pi5.yml up --build"
