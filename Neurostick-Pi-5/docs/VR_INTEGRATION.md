# Pi 5 → VR Integration

The Pi 5 edge service can either be **polled** by the VR / robot middleware
over plain HTTP, or it can **push** state to subscribers via Server-Sent
Events. Use the push channel for anything that drives a control loop; use
polling only for one-off queries.

| Endpoint | Method | Purpose | Latency |
|---|---|---|---|
| `/health`            | GET  | liveness | ms |
| `/status`            | GET  | full state snapshot | ms |
| `/snapshot`          | GET  | rolling 4 s of every channel (heavy ~ 50 KB) | ms |
| `/decision`          | GET  | latest SSVEP decision only | ms |
| `/connect` `/start` `/stop`         | POST | session control | ms |
| `/record/start` `/record/stop`      | POST | open / close a recording session on disk | ms |
| **`/events`** | GET  | **Server-Sent Events stream**, ~20 Hz frames | push |

CORS is enabled on every endpoint (`Access-Control-Allow-Origin: *`) so a
browser-based VR frontend (WebXR / Three.js / Unity WebGL) can connect
directly.

## Event frame contract (`/events`)

Each `data:` line is one `EventFrame` JSON object:

```json
{
  "t_unix": 1777333054.218,
  "connected": true,
  "streaming": true,
  "simulating": false,
  "sample_rate_hz": 125.0,
  "eeg_channels": 16,
  "signal_quality": "ok",
  "railed_channels": 0,
  "signal_hint": null,
  "latest_decision": {
    "best_freq_hz": 12.0,
    "confident": true,
    "margin": 0.40,
    "scores": [[12.0, 0.50], [15.0, 0.10], [8.0, 0.005], [20.0, 0.005]]
  },
  "last_sample": [-0.46, 0.12, /* ... 16 µV values ... */],
  "last_error": null
}
```

- `t_unix` — wall-clock seconds since UNIX epoch on the Pi when the frame was
  built. Use it to detect dropped frames or to align with external sensors.
- `signal_quality` — **trust gate. Read this before `latest_decision`.**
  - `"ok"` — µV in a plausible range, decision is usable.
  - `"railed"` — most channels pinned at the ADS1299 ±187500 µV full scale.
    Electrodes are not contacting scalp, or the BIAS/SRB reference is not
    connected. `latest_decision` is force-downgraded to
    `best_freq_hz: null, confident: false` so a saturated headset cannot
    drive your VR cursor with garbage. `railed_channels` tells you how many
    of the channels are bad; `signal_hint` is a human string for the HUD.
  - `"no_signal"` — nothing buffered yet (not connected / not streaming).
- `railed_channels` — count of channels stuck at the rail in the recent
  window. `0` when healthy.
- `signal_hint` — `null` when `signal_quality == "ok"`, otherwise a
  human-readable Chinese explanation suitable for surfacing on the operator
  HUD.
- `latest_decision` — same `SsvepDecision` object that `/decision` returns.
  `null` until enough samples have accumulated (the first few hundred ms
  after `/start`). Bind your VR cursor or robot pose target to
  `best_freq_hz` only when `confident == true` **and**
  `signal_quality == "ok"`.
- `last_sample` — most recent µV reading per channel (`sample_rate_hz` rate
  is preserved on the wire, but only the most recent sample is included to
  keep frame size bounded). For waveform plotting, use `/snapshot` once
  every couple of seconds and interpolate.
- `last_error` — non-null if the worker hit a transient acquisition error;
  surface it to the VR HUD.

Push cadence is **~20 Hz** (every 5th sample at 125 Hz, every 12th at 250
Hz). A `: keep-alive` SSE comment is emitted every 15 s if there is no
real traffic, so reverse proxies do not time out the socket.

## JavaScript / WebXR / Unity WebGL

```js
// Browser / WebXR
const events = new EventSource("http://<pi-ip>:8765/events");
events.onmessage = (msg) => {
  const frame = JSON.parse(msg.data);
  if (!frame.streaming) return;
  if (frame.signal_quality !== "ok") {
    // Headset railed or not connected — show the hint, don't act on decision.
    showHudWarning(frame.signal_hint);
    return;
  }
  if (frame.latest_decision?.confident) {
    const target = frame.latest_decision.best_freq_hz;
    // map 8/12/15/20 Hz -> action; e.g. left / forward / right / select
    dispatchIntent(target);
  }
  // Optional: drive a waveform meter from last_sample[6] (= O1)
};
events.onerror = (err) => {
  // Auto-reconnects per SSE spec; log and let it retry.
  console.warn("Pi events stream interrupted, retrying…", err);
};
```

EventSource handles reconnection on transport drop automatically.

## Unity (engine, not WebGL)

`UnityEngine.Networking.UnityWebRequest` does not natively support SSE. Two
options:

1. **Treat it as a chunked-encoded HTTP stream** with `DownloadHandlerScript`
   and split on `\n\n`. ~30 lines.
2. **Subscribe via a small bridge process** (Node / Python) that translates
   SSE → UDP datagrams on `localhost`, and consume UDP from Unity. This is
   the path most XR projects take.

If you take option 2, the bridge is one line:

```bash
curl -N http://<pi-ip>:8765/events | \
  while IFS= read -r line; do
    case "$line" in
      data:*) echo "${line#data: }" | nc -u 127.0.0.1 9001 ;;
    esac
  done
```

Inside Unity:

```csharp
var udp = new UdpClient(9001);
while (!cancel.IsCancellationRequested) {
    var pkt = udp.Receive(ref ep);
    var frame = JsonUtility.FromJson<EventFrame>(Encoding.UTF8.GetString(pkt));
    if (frame.streaming && frame.latest_decision != null && frame.latest_decision.confident) {
        DispatchIntent(frame.latest_decision.best_freq_hz);
    }
}
```

## Unreal Engine

Use `FHttpModule::Get().CreateRequest()` with `OnRequestProgress` to read
the chunked stream incrementally. Same `\n\n` delimiter as the JS version.

## Native C / C++ (e.g. ROS2 bridge to a robotic arm)

Use libcurl with `CURLOPT_WRITEFUNCTION`:

```c
size_t on_chunk(char *data, size_t size, size_t n, void *user_data) {
    /* parse `data: ...\n\n` and forward to ROS2 / DDS / shared memory */
    return size * n;
}
curl_easy_setopt(curl, CURLOPT_URL, "http://<pi-ip>:8765/events");
curl_easy_setopt(curl, CURLOPT_WRITEFUNCTION, on_chunk);
curl_easy_perform(curl);
```

## Operational notes

- **Multiple subscribers are supported.** The Pi runs one worker thread that
  fans events out to every connected `/events` client. A VR headset, a
  log-tap, and a robot bridge can all attach simultaneously.
- **No back-pressure today.** If a slow client cannot drain its socket fast
  enough, its mpsc channel will buffer in memory. Disconnect lossy clients;
  do not pipe `/events` into something CPU-heavy on the Pi itself.
- **Wire format is stable.** New fields may be added to `EventFrame`; clients
  must ignore unknown JSON keys.
- **No authentication.** Bind the Pi behind a trusted LAN segment, or front
  it with an SSH tunnel / WireGuard for remote access.
