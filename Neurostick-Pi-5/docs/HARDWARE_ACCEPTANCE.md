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
