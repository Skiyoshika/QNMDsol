#!/usr/bin/env bash
set -euo pipefail

BASE_URL="${BASE_URL:-http://127.0.0.1:8765}"
curl -fsS "$BASE_URL/health"
echo
curl -fsS "$BASE_URL/status"
echo
