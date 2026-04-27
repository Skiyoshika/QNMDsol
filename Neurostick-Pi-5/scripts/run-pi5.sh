#!/usr/bin/env bash
set -euo pipefail

cd "$(dirname "$0")/.."
: "${OPENBCI_DEVICE:?Set OPENBCI_DEVICE=/dev/serial/by-id/<dongle>}"
docker compose -f docker-compose.pi5.yml up --build
