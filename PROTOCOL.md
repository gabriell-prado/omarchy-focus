# Focus engine protocol (version 1)

The Omarchy panel and the `omarchy-focus` CLI are clients of the engine. Any
other UI or script can be one too.

## Transport

- Unix socket: `$XDG_RUNTIME_DIR/omarchy-focus.sock` (mode 0600).
- One JSON object per line, in both directions, UTF-8, `\n`-terminated.
  Requests longer than 64 KiB are refused.
- Every request gets exactly one reply line: `{"ok":true, ...}` or
  `{"ok":false,"error":"..."}`. A subscribed connection also receives event
  lines (`{"event": ...}`), which can arrive before, between or after replies.
  Tell them apart by the `ok` / `event` key.

Try it by hand:

```bash
printf '{"cmd":"status"}\n' | socat - UNIX-CONNECT:"$XDG_RUNTIME_DIR/omarchy-focus.sock"
```

## Requests

`{"cmd": "<name>", ...}`. An optional `"v": 1` is accepted and ignored.

| cmd | fields | effect |
|---|---|---|
| `subscribe` | | keep this connection: `catalog` once, then `state` on every change |
| `status` | | reply carries `state` |
| `toggle` `start` `pause` `reset` `skip` | | timer control (`reset` also restarts the set; today's count is kept) |
| `adjust` | `minutes`: number (±360) | add or take away time from the session in progress (running or paused); a running session cut to zero ends with its alert. Error `no session in progress` when idle or the next phase has not started |
| `set` | `patch`: object | change settings; unknown keys and invalid values are an error, numbers are clamped to their range |
| `preset` | `kind`: `timer`\|`sound`, `id` | apply a preset; a sound preset also switches the sound on |
| `sound` | `on`: bool (optional) | switch the focus sound on/off; toggles without `on` |
| `audition` | `on`: bool | while on, a sound that follows the timer also plays outside focus sessions; ends when this connection closes |
| `alert` | `id` | play an alert, cutting off any alert still sounding; `none` stops |
| `stop-alert` | | stop the alert |

## Events

### `catalog`

Sent once per subscription. Everything the UI lists comes from here, so it
holds no domain knowledge of its own.

```json
{"event":"catalog","catalog":{
  "version": 1,
  "timerPresets": [{"id":"classic","focus":25,"short":5,"long":15,"sessions":4,"refs":"…"}],
  "customTimerRefs": "Ariga & Lleras 2011",
  "soundPresets": [{"id":"speech","evidence":"moderate","noiseColor":"pink",
                    "beatMode":"off","beatHz":0,"carrierHz":0,"refs":"…"}],
  "alerts": ["none","chime","bowl","beep","rise","complete","bell","message","alarm","attention","login","phone","custom"],
  "noiseColors": ["off","white","pink","brown","custom"],
  "beatModes": ["off","binaural","isochronic"],
  "bands": [["delta",0],["theta",4],["alpha",8],["beta",13],["gamma",30]],
  "eqBands": [31.5,63,125,250,500,1000,2000,4000,8000,16000],
  "eqShapes": {"white":[21,25,30,35,42,50,60,71,84,100],"pink":[100,"…"],"brown":[100,84,"…",21]},
  "limits": {"focusMinutes":[1,180], "…": "…"}
}}
```

`eqBands` are the custom noise's octave band centres (Hz) and `eqShapes` the
`noiseBands` levels (0–100) that sound like each named color, so an
equalizer UI can show the playing color's shape and start a custom noise
from it.

Preset and alert names and notes are translated by the client from its i18n
catalog (`preset.<id>.name`, `preset.<id>.note`, `evidence.<level>`,
`timer.preset.<id>.name`, `alert.<id>`, …); citations (`refs`) are not
translated.

### `state`

Sent on subscription and whenever something a client can see changes. It is
not sent once a second: while `running`, derive the countdown from `endsAt`
(ms since the Unix epoch, same clock as the client).

```json
{"event":"state","state":{
  "phase": "idle|focus|short|long",
  "running": false, "fresh": true, "paused": false,
  "endsAt": 0, "leftMs": 1500000, "totalMs": 1500000, "progress": 0.0,
  "session": 1, "sessions": 4, "doneInSet": 0, "today": 0,
  "settings": { "focusMinutes": 25, "…": "every setting" },
  "timerPreset": "classic", "soundPreset": "speech",
  "sound": {"wanted": false, "active": false, "auditioning": false, "playing": false, "failed": false}
}}
```

- `fresh`: the phase is queued but not started; its length follows the settings.
- `paused`: a started phase that is not running.
- `session`: 1-based position in the set; `doneInSet`: finished focus sessions in it.
- `timerPreset` / `soundPreset`: the matching preset id, or `custom`.
- `sound.wanted`: the settings say it should play now; `active`: it should
  play (wanted, or auditioned); `playing`: it is being heard; `failed`: the
  audio output could not be opened.
