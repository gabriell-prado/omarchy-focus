# Focus: an Omarchy shell plugin

A Pomodoro timer in the bar, with **research-based focus sounds** (white, pink
and brown noise, binaural and isochronic beats) and **selectable alert
sounds**. Every preset shows a plain-language summary of the research, how strong
the evidence is, and the citations.

Everything is generated live with `ffmpeg` (no audio files to download) and
played through PipeWire as the streams **Focus noise**, **Focus tone** and
**Focus alert**, so they show up in the audio panel like any other app.

The focus sound is continuous: once on, it plays without stopping until you
turn it off, unless *Only during focus sessions* links it to the timer.
Volume changes apply live to the streams (no restart, no gap), and changing
the sound itself crossfades from the old generator to the new one.

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

### Pages

- **Timer**: method presets (25/5, 50/10, 52/17, 90/20) with the evidence for
  each, sliders for focus / short break / long break / sessions per set, and
  auto-start switches.
- **Sound**: master switch, seven research-based presets, the evidence card,
  noise and tone volume, and *Only during focus sessions* (default on: sound
  starts with a focus session and stops for pauses and breaks). Picking a
  preset or changing the sound switches it on. When it is linked to the timer
  and no focus session is running, the change is previewed until you close
  the panel.
- **Tune**: noise color, brainwave tone (binaural / isochronic), beat frequency
  with its EEG band, and carrier tone.
- **Alerts**: separate sounds for *focus ends* and *break ends* (5 synthesized
  tones + 7 freedesktop sounds + a custom file). The arrows choose and play
  the previous/next sound; clicking the name plays it again. Only one alert
  plays at a time: a new one cuts off the one still sounding. Also volume,
  desktop notifications, and whether to show the time in the bar.

### Keyboard shortcuts and scripts

The engine answers IPC on the `focus` target:

```bash
omarchy-shell focus toggle    # start / pause
omarchy-shell focus start | pause | reset | skip
omarchy-shell focus sound     # focus sound on / off
omarchy-shell focus status    # JSON: phase, running, left, session, today, sound
omarchy-shell focus alert bowl     # play an alert sound (stops any playing alert)
omarchy-shell local.focus toggle   # open / close the panel
```

To bind them, add to `~/.config/hypr/bindings.lua`. Check the keys are free
first:

```lua
o.bind("SUPER + ALT + P", "Focus timer", "omarchy-shell focus toggle")
o.bind("SUPER + ALT + N", "Focus sound", "omarchy-shell focus sound")
```

## Settings

All settings live in this widget's entry in `~/.config/omarchy/shell.json`.
Only values that differ from the defaults are written. You can change them in
the panel, in Omarchy's settings, or with `omarchy bar set local.focus <key> <value> --json`.

| Key | Default | Range |
|---|---|---|
| `focusMinutes` / `shortBreakMinutes` / `longBreakMinutes` | 25 / 5 / 15 | 1–180 / 1–60 / 1–90 |
| `sessionsBeforeLongBreak` | 4 | 1–12 |
| `autoStartBreaks` / `autoStartFocus` | true / false | |
| `soundEnabled` / `soundOnlyDuringFocus` | false / true | |
| `noiseColor` | `pink` | `off` `white` `pink` `brown` |
| `noiseVolume` / `toneVolume` | 50 / 35 | 0–100 (square-law curve) |
| `beatMode` | `off` | `off` `binaural` `isochronic` |
| `beatHz` / `carrierHz` | 16 / 250 | 1–45 / 100–600 |
| `focusEndSound` / `breakEndSound` | `bowl` / `chime` | `none` `chime` `bowl` `beep` `rise` `complete` `bell` `message` `alarm` `attention` `login` `phone` `custom` |
| `alertVolume` | 70 | 0–100 |
| `customSoundPath` | `""` | absolute or `~/` path; played for at most 30 s |
| `desktopNotifications` / `showTimeInBar` | true / true | |

Timer progress is kept in `~/.local/state/omarchy-focus/state.json`, so a
shell restart or plugin reload resumes a running session. A session that ended
while the shell was down more than a minute ago moves on without an alert.

## The science, and how strong it is

Loudness: all three noise colors are calibrated to about −16 LUFS at 100 %,
so switching color does not jump in volume. Your system volume still sets
the actual level, so keep it moderate.

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

## Files

```
manifest.json      plugin manifest + settings schema
Service.qml        the single engine: timer, sound process, alerts, IPC, state
Panel.qml          bar widget + panel (one per monitor; draws the service)
Model.js           pure logic: settings, presets, ffmpeg command builders
i18n/en.json       English strings (pt-BR.json: Brazilian Portuguese)
tests/model.test.mjs   node tests; --ffmpeg also renders every sound to a null sink
```

Run the tests: `node ~/.config/omarchy/plugins/focus/tests/model.test.mjs --ffmpeg`

After editing the QML or JS, run `omarchy restart shell`: the plugin hot-reload
rebuilds the widget but reuses the previously compiled code. A running session
survives the restart.

Requires `ffmpeg` (with PulseAudio output, via pipewire-pulse), `setpriv`
(util-linux) and `notify-send`, all present on Omarchy. The sound generator is
started with a parent-death signal, so it cannot keep playing if the shell dies.
