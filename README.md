# Focus: Pomodoro and focus sounds for Omarchy

A Pomodoro timer in the bar, with **research-based focus sounds** (white, pink
and brown noise, binaural and isochronic beats) and **selectable alert
sounds**. Every preset shows a plain-language summary of the research, how
strong the evidence is, and the citations.

## Architecture

```
┌────────────── Omarchy shell (Quickshell, QML) ────────────┐
│  Panel.qml + ui/   bar widget and panel, one per monitor  │
│  Service.qml       one socket client shared by all panels │
└──────────────────────────────┬────────────────────────────┘
                               │  $XDG_RUNTIME_DIR/omarchy-focus.sock
                               │  newline-delimited JSON (PROTOCOL.md)
┌──────────────────────────────┴────────────────────────────┐
│  omarchy-focus daemon (Rust, systemd user service)        │
│    timer state machine · settings · presets · state file  │
│    audio: noise / tone / alert synthesis → mixer → Pulse  │
│    desktop notifications · i18n · CLI                     │
└───────────────────────────────────────────────────────────┘
```

The shell only loads plugin UI written in QML, so the bar widget and panel
stay QML; they are a thin view and hold no logic of their own. Everything else
is the **Focus engine**, a small Rust daemon:

- The timer and the sound keep running through shell restarts and plugin
  reloads. A daemon restart resumes the session from its state file.
- Audio is synthesised in-process and played as one PulseAudio/PipeWire
  stream, **Focus**. The volume and tone sliders are heard while you drag
  them: volume and frequency changes ramp in 80 ms, a different sound
  crossfades over 400 ms, stopping fades out, and only one alert plays at a
  time. Nothing is restarted and nothing clicks.
- One source of truth for settings (`~/.config/omarchy-focus/config.toml`),
  shared by the panel, the CLI and hand edits.
- A CLI, `omarchy-focus`, that works with or without the shell.

## Install

Requires the Rust toolchain (`cargo`) and PulseAudio's client library (on
Omarchy it comes with PipeWire). From this folder:

```bash
make install            # build, install ~/.local/bin/omarchy-focus, enable the user service
omarchy restart shell   # load the panel
```

`make test` runs the engine's tests, `make logs` follows its log, and
`make uninstall` removes the binary and the service. Builds go to
`~/.cache/omarchy-focus`, outside this folder, because the shell reloads the
plugin whenever a file inside it changes.

If the engine is not running, the panel says so and offers to start it.

## Using it

| Where | Action |
|---|---|
| Bar icon, left click | open the panel |
| Bar icon, right click | start / pause |
| Bar icon, middle click | focus sound on / off |
| Panel: `Space` / `Enter` | start / pause |
| Panel: `r` · `s` · `n` | reset · skip · sound on/off |
| Panel: `1`–`4`, `←` `→` | Timer · Sound · Tune · Alerts pages |

The bar shows the remaining time while a session runs. The icon changes for
focus (󰔛), breaks (󰅶) and pause (󰏤).

- **Timer**: method presets (25/5, 50/10, 52/17, 90/20) with the evidence for
  each, sliders for focus / short break / long break / sessions per set, and
  auto-start switches.
- **Sound**: master switch, seven research-based presets, the evidence card,
  noise and tone volume, and *Only during focus sessions* (sound starts with a
  focus session and stops for pauses and breaks). Otherwise the sound is
  continuous until you switch it off. Picking a preset or changing the sound
  switches it on; when it follows the timer and no focus session is running,
  the change is previewed until you close the panel.
- **Tune**: noise color, a ten-band equalizer to shape the noise by hand, the
  brainwave tone (binaural / isochronic), beat frequency with its EEG band,
  and carrier tone.

### Shaping the noise

The Tune page has one vertical slider per octave, 31 Hz to 16 kHz. While a
color is playing they show its shape; moving one switches to a **Custom**
noise that starts from what you hear. It is pink noise split into ten octave
bands, each from its own generator, cut out by 4th-order Butterworth edges:
equal sliders are pink (flat to 0.02 dB), a rising shape is brighter, a
falling one darker, and a band at zero is silent without touching its
neighbours. The engine keeps every shape exactly as loud as pink (matched in
K-weighted loudness, like a LUFS meter), so the sliders change the color and
the volume slider stays in charge of the level. Changes are heard while you
drag, gliding over 80 ms.
- **Alerts**: separate sounds for *focus ends* and *break ends* (4 synthesised
  tones, 7 freedesktop sounds, or a custom file). The arrows choose and play
  the previous/next sound; clicking the name plays it again. Also volume,
  desktop notifications, and whether to show the time in the bar.

### Command line

```bash
omarchy-focus toggle | start | pause | reset | skip
omarchy-focus sound [on|off|toggle]
omarchy-focus preset timer classic|deep|desktime|ultradian
omarchy-focus preset sound speech|white|brown|beta|gamma|alpha|iso40
omarchy-focus set focusMinutes 50          # any setting below
omarchy-focus alert bowl                   # play an alert (none stops it)
omarchy-focus status [--full]              # JSON
omarchy-shell local.focus toggle           # open / close the panel
```

Keyboard shortcuts, in `~/.config/hypr/bindings.lua` (check the keys are free):

```lua
o.bind("SUPER + ALT + P", "Focus timer", "omarchy-focus toggle")
o.bind("SUPER + ALT + N", "Focus sound", "omarchy-focus sound")
```

## Settings

`~/.config/omarchy-focus/config.toml`, written by the panel and
`omarchy-focus set`; hand edits are picked up within a second. On its first
start the engine imports the settings the earlier QML-only version kept in
`shell.json`.

| Key | Default | Range |
|---|---|---|
| `focusMinutes` / `shortBreakMinutes` / `longBreakMinutes` | 25 / 5 / 15 | 1–180 / 1–60 / 1–90 |
| `sessionsBeforeLongBreak` | 4 | 1–12 |
| `autoStartBreaks` / `autoStartFocus` | true / false | |
| `soundEnabled` / `soundOnlyDuringFocus` | false / true | |
| `noiseColor` | `pink` | `off` `white` `pink` `brown` `custom` |
| `noiseBands` | ten × 100 | 0–100 per octave band (31 Hz … 16 kHz), used by `custom` |
| `noiseVolume` / `toneVolume` | 50 / 35 | 0–100 (square-law curve) |
| `beatMode` | `off` | `off` `binaural` `isochronic` |
| `beatHz` / `carrierHz` | 16 / 250 | 1–45 / 100–600 |
| `focusEndSound` / `breakEndSound` | `bowl` / `chime` | `none` `chime` `bowl` `beep` `rise` `complete` `bell` `message` `alarm` `attention` `login` `phone` `custom` |
| `alertVolume` | 70 | 0–100 |
| `customSoundPath` | `""` | absolute or `~/` path (ogg, mp3, flac, wav); at most 30 s |
| `desktopNotifications` / `showTimeInBar` | true / true | |

Timer progress is kept in `~/.local/state/omarchy-focus/state.json`. A
session that ended more than a minute before the engine started moves on
without an alert.

## The science, and how strong it is

Loudness: the three noise colors play at the same loudness (about −13 LUFS
stereo at 100 %, measured with EBU R128), so switching color does not jump in
volume. Your system volume still sets the actual level, so keep it moderate.

| Preset | Sound | Evidence | Basis |
|---|---|---|---|
| Speech masking | pink noise | moderate | Irrelevant-speech effect: nearby speech disrupts reading/writing; masking lowers speech intelligibility and its cost. |
| Attention boost | white noise | moderate (population-specific) | White noise improved memory in inattentive/ADHD children and slightly impaired attentive ones (moderate brain arousal model). |
| Deep brown | brown noise | limited | Same masking principle with less high-frequency energy; popular, little direct research. |
| Beta vigilance | pink + 16 Hz binaural | limited | Beta beats in pink noise improved vigilance and mood vs 1.5–4 Hz beats (small study). |
| Gamma attention | pink + 40 Hz binaural | limited | 40 Hz beats reduced the attentional blink, only in some listeners (dopamine-dependent). |
| Alpha calm | brown + 10 Hz binaural | limited | Meta-analysis: binaural beats affect memory, attention, anxiety, with high heterogeneity. |
| 40 Hz pulses | brown + 40 Hz isochronic | experimental | 40 Hz sensory stimulation (GENUS) benefits shown in mice; unproven for healthy human cognition. |

Caveats the panel also states: binaural beats need headphones; beats are only
heard with carriers below about 1000 Hz; whether beats *entrain* brainwaves is
debated (EEG findings are inconsistent).

Timer methods: Pomodoro 25/5 (Cirillo; fixed breaks lowered fatigue vs
self-chosen breaks, Biwer et al. 2023); 50/10 (untested variant); 52/17
(DeskTime company data, not peer-reviewed); 90/20 (ultradian rhythm, weak
evidence for knowledge work). Brief planned breaks prevent vigilance decrements
(Ariga & Lleras 2011).

### References

- Ariga, A., & Lleras, A. (2011). Brief and rare mental "breaks" keep you focused. *Cognition*, 118(3), 439–443.
- Biwer, F., Wiradhany, W., oude Egbrink, M. G. A., & de Bruin, A. B. H. (2023). Understanding effort regulation: Comparing "Pomodoro" breaks and self-regulated breaks. *British Journal of Educational Psychology*, 93(S2), 353–367.
- Cirillo, F. (2006). *The Pomodoro Technique*.
- Colle, H. A., & Welsh, A. (1976). Acoustic masking in primary memory. *Journal of Verbal Learning and Verbal Behavior*, 15(1), 17–31.
- Garcia-Argibay, M., Santed, M. A., & Reales, J. M. (2019). Efficacy of binaural auditory beats in cognition, anxiety, and pain perception: a meta-analysis. *Psychological Research*, 83(2), 357–372.
- Hongisto, V. (2005). A model predicting the effect of speech of varying intelligibility on work performance. *Indoor Air*, 15(6), 458–468.
- Kleitman, N. (1982). Basic rest-activity cycle: 22 years later. *Sleep*, 5(4), 311–317.
- Lane, J. D., Kasian, S. J., Owens, J. E., & Marsh, G. R. (1998). Binaural auditory beats affect vigilance performance and mood. *Physiology & Behavior*, 63(2), 249–252.
- Licklider, J. C. R., Webster, J. C., & Hedlun, J. M. (1950). On the frequency limits of binaural beats. *JASA*, 22(4), 468–473.
- Martorell, A. J., et al. (2019). Multi-sensory gamma stimulation ameliorates Alzheimer's-associated pathology and improves cognition. *Cell*, 177(2), 256–271.
- Mehta, R., Zhu, R., & Cheema, A. (2012). Is noise always bad? Exploring the effects of ambient noise on creative cognition. *Journal of Consumer Research*, 39(4), 784–799.
- Reedijk, S. A., Bolders, A., Colzato, L. S., & Hommel, B. (2015). Eliminating the attentional blink through binaural beats: a case for tailored cognitive enhancement. *Frontiers in Psychiatry*, 6, 82.
- Söderlund, G., Sikström, S., & Smart, A. (2007). Listen to the noise: noise is beneficial for cognitive performance in ADHD. *Journal of Child Psychology and Psychiatry*, 48(8), 840–847.
- Söderlund, G. B. W., Sikström, S., Loftesnes, J. M., & Sonuga-Barke, E. J. (2010). The effects of background white noise on memory performance in inattentive school children. *Behavioral and Brain Functions*, 6, 55.
- WHO & ITU (2019). *H.870: Safe listening devices and systems*.

## Development

```
manifest.json                 plugin manifest (service + bar widget)
Panel.qml                     bar button, panel frame, hero, controls, page tabs
Service.qml                   engine client: socket, reconnect, local countdown
ui/                           Context (shared helpers) · pages · controls
Model.js                      UI helpers (glyphs, clock format)
i18n/en.json, pt-BR.json      strings, shared by the panel and the engine
PROTOCOL.md                   the socket protocol
daemon/                       the engine (Rust crate `omarchy-focus`)
  src/core.rs                   owner of state: commands, ticks, broadcasts
  src/timer.rs                  Pomodoro state machine (pure, tested)
  src/settings.rs, presets.rs   typed settings, presets and evidence
  src/audio/                    noise, tone, alerts, mixer, output
  tests/daemon.rs               end-to-end tests against the real binary
systemd/omarchy-focus.service user service
Makefile                      build · test · install · uninstall
```

- `omarchy-focus render --preset gamma --seconds 10 --out /tmp/x.wav` renders
  a sound offline (also `--noise <color>`, `--alert <id>`), for listening or
  measuring with `ffmpeg -af ebur128`.
- After editing QML, run `omarchy restart shell`: the shell's hot reload
  rebuilds the widget but reuses previously compiled code. The engine keeps
  playing through it.
- After editing the engine, `make install` rebuilds and restarts it; the
  running session resumes.
