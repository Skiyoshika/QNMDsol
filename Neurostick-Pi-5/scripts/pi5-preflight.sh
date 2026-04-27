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
