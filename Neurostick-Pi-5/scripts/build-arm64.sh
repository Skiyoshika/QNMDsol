#!/usr/bin/env bash
set -euo pipefail

cd "$(dirname "$0")/../.."
docker buildx build \
  --platform linux/arm64 \
  -f Neurostick-Pi-5/Dockerfile \
  -t neurostick-pi5-edge:local \
  --load \
  .
