.pragma library

// Pure logic for the Focus plugin: settings, presets, the Pomodoro state
// machine's arithmetic, and the ffmpeg command lines that make every sound.
// Nothing here touches QML objects, so tests/model.test.mjs can run it in node.

var PLUGIN_ID = "local.focus"
var IPC_TARGET = "focus"
var SAMPLE_RATE = 48000

var DEFAULTS = {
  focusMinutes: 25,
  shortBreakMinutes: 5,
  longBreakMinutes: 15,
  sessionsBeforeLongBreak: 4,
  autoStartBreaks: true,
  autoStartFocus: false,

  soundEnabled: false,
  soundOnlyDuringFocus: true,
  noiseColor: "pink",
  noiseVolume: 50,
  beatMode: "off",
  beatHz: 16,
  carrierHz: 250,
  toneVolume: 35,

  focusEndSound: "bowl",
  breakEndSound: "chime",
  alertVolume: 70,
  customSoundPath: "",
  desktopNotifications: true,
  showTimeInBar: true
}

var INT_LIMITS = {
  focusMinutes: [1, 180],
  shortBreakMinutes: [1, 60],
  longBreakMinutes: [1, 90],
  sessionsBeforeLongBreak: [1, 12],
  noiseVolume: [0, 100],
  beatHz: [1, 45],
  carrierHz: [100, 600],
  toneVolume: [0, 100],
  alertVolume: [0, 100]
}

var NOISE_COLORS = ["off", "white", "pink", "brown"]
var BEAT_MODES = ["off", "binaural", "isochronic"]

// anoisesrc amplitudes that put each color at about -16 LUFS, measured with
// ffmpeg's ebur128 filter, so switching color does not jump in loudness.
var NOISE_AMPLITUDE = { white: 0.19, pink: 0.86, brown: 0.95 }
// Peak amplitude of a tone at 100 % before the volume curve.
var TONE_AMPLITUDE = 0.25

// ---------------------------------------------------------------- presets

// Durations in minutes. The note for each lives in i18n as
// timer.preset.<id>.note so the evidence reads in the user's language.
var TIMER_PRESETS = [
  { id: "classic", focus: 25, short: 5, long: 15, sessions: 4, refs: "Cirillo 2006 · Biwer et al. 2023" },
  { id: "deep", focus: 50, short: 10, long: 30, sessions: 3, refs: "" },
  { id: "desktime", focus: 52, short: 17, long: 17, sessions: 4, refs: "DeskTime 2014 (company data)" },
  { id: "ultradian", focus: 90, short: 20, long: 30, sessions: 2, refs: "Kleitman 1982" }
]
var CUSTOM_TIMER_REFS = "Ariga & Lleras 2011"

function timerRefs(id) {
  for (var i = 0; i < TIMER_PRESETS.length; i++) if (TIMER_PRESETS[i].id === id) return TIMER_PRESETS[i].refs
  return CUSTOM_TIMER_REFS
}

// Evidence: "moderate" (replicated or meta-analytic support), "limited"
// (single or small studies, or indirect support), "experimental" (mostly
// animal work or mechanism only). Citations are kept verbatim, untranslated.
var SOUND_PRESETS = [
  { id: "speech", evidence: "moderate", noiseColor: "pink", noiseVolume: 50, beatMode: "off",
    refs: "Colle & Welsh 1976 · Hongisto 2005" },
  { id: "white", evidence: "moderate", noiseColor: "white", noiseVolume: 45, beatMode: "off",
    refs: "Söderlund et al. 2007, 2010" },
  { id: "brown", evidence: "limited", noiseColor: "brown", noiseVolume: 55, beatMode: "off",
    refs: "" },
  { id: "beta", evidence: "limited", noiseColor: "pink", noiseVolume: 45, beatMode: "binaural",
    beatHz: 16, carrierHz: 250, toneVolume: 40, refs: "Lane et al. 1998" },
  { id: "gamma", evidence: "limited", noiseColor: "pink", noiseVolume: 40, beatMode: "binaural",
    beatHz: 40, carrierHz: 400, toneVolume: 35, refs: "Reedijk et al. 2015 · Garcia-Argibay et al. 2019" },
  { id: "alpha", evidence: "limited", noiseColor: "brown", noiseVolume: 45, beatMode: "binaural",
    beatHz: 10, carrierHz: 200, toneVolume: 40, refs: "Garcia-Argibay et al. 2019" },
  { id: "iso40", evidence: "experimental", noiseColor: "brown", noiseVolume: 35, beatMode: "isochronic",
    beatHz: 40, carrierHz: 300, toneVolume: 30, refs: "Martorell et al. 2019" }
]

// ---------------------------------------------------------------- alerts

var FREEDESKTOP_SOUNDS = "/usr/share/sounds/freedesktop/stereo/"

// A struck note: a sine plus a quieter octave, decaying from `start` seconds.
function _note(freq, start, amp, decay) {
  var s = String(start)
  var dt = "(t-" + s + ")"
  return "gte(t," + s + ")*" + amp + "*(sin(2*PI*" + freq + "*" + dt + ")+0.35*sin(4*PI*" + freq + "*" + dt
    + "))*exp(-" + decay + "*" + dt + ")"
}

var ALERT_SOUNDS = [
  { id: "none" },
  { id: "chime", duration: 2.8,
    expr: _note(659.25, 0, 0.3, 2.8) + "+" + _note(987.77, 0.3, 0.3, 2.8) },
  { id: "bowl", duration: 6,
    expr: "min(1,t*30)*(0.3*sin(2*PI*196*t)*exp(-0.5*t)+0.25*sin(2*PI*197.3*t)*exp(-0.5*t)"
      + "+0.14*sin(2*PI*539*t)*exp(-0.9*t)+0.07*sin(2*PI*1013*t)*exp(-1.5*t))" },
  { id: "beep", duration: 0.72,
    expr: "0.3*sin(2*PI*880*t)*lt(mod(t,0.24),0.13)*sin(PI*mod(t,0.24)/0.13)" },
  { id: "rise", duration: 2.6,
    expr: [_note(523.25, 0, 0.2, 3), _note(659.25, 0.14, 0.2, 3), _note(783.99, 0.28, 0.2, 3), _note(1046.5, 0.42, 0.2, 3)].join("+") },
  { id: "complete", file: FREEDESKTOP_SOUNDS + "complete.oga" },
  { id: "bell", file: FREEDESKTOP_SOUNDS + "bell.oga" },
  { id: "message", file: FREEDESKTOP_SOUNDS + "message.oga" },
  { id: "alarm", file: FREEDESKTOP_SOUNDS + "alarm-clock-elapsed.oga" },
  { id: "attention", file: FREEDESKTOP_SOUNDS + "window-attention.oga" },
  { id: "login", file: FREEDESKTOP_SOUNDS + "service-login.oga" },
  { id: "phone", file: FREEDESKTOP_SOUNDS + "phone-incoming-call.oga" },
  { id: "custom" }
]

var ALERT_IDS = ALERT_SOUNDS.map(function(s) { return s.id })

var ENUMS = {
  noiseColor: NOISE_COLORS,
  beatMode: BEAT_MODES,
  focusEndSound: ALERT_IDS,
  breakEndSound: ALERT_IDS
}

// ---------------------------------------------------------------- settings

function clampInt(value, lo, hi, fallback) {
  var n = Math.round(Number(value))
  if (value === null || value === undefined || value === "" || !isFinite(n)) return fallback
  return Math.max(lo, Math.min(hi, n))
}

// Every key present, every value valid. Unknown keys are dropped.
function normalizeSettings(raw) {
  var src = raw && typeof raw === "object" ? raw : {}
  var out = {}
  for (var key in DEFAULTS) {
    var fallback = DEFAULTS[key]
    var value = src[key]
    if (INT_LIMITS[key]) out[key] = clampInt(value, INT_LIMITS[key][0], INT_LIMITS[key][1], fallback)
    else if (ENUMS[key]) out[key] = ENUMS[key].indexOf(String(value)) !== -1 ? String(value) : fallback
    else if (typeof fallback === "boolean") out[key] = typeof value === "boolean" ? value : fallback
    else out[key] = typeof value === "string" ? value.slice(0, 1024) : fallback
  }
  return out
}

// Only what differs from the defaults, so shell.json stays short.
function compactSettings(full) {
  var normalized = normalizeSettings(full)
  var out = {}
  for (var key in DEFAULTS) if (normalized[key] !== DEFAULTS[key]) out[key] = normalized[key]
  return out
}

// This plugin's entry in a bar config ({ layout: { section: [entries] } }).
function findEntry(barConfig, id) {
  if (!barConfig || typeof barConfig !== "object") return null
  var layout = barConfig.layout
  if (!layout || typeof layout !== "object") return null
  for (var section in layout) {
    var entries = layout[section]
    if (!Array.isArray(entries)) continue
    for (var i = 0; i < entries.length; i++) {
      var entry = entries[i]
      if (entry && typeof entry === "object" && String(entry.id || "") === id) return entry
    }
  }
  return null
}

// ---------------------------------------------------------------- timer

function phaseMinutes(phase, cfg) {
  if (phase === "short") return cfg.shortBreakMinutes
  if (phase === "long") return cfg.longBreakMinutes
  return cfg.focusMinutes
}

function phaseMs(phase, cfg) {
  return phaseMinutes(phase, cfg) * 60000
}

function isBreak(phase) {
  return phase === "short" || phase === "long"
}

// `completed` already counts the focus session that just ended.
function breakAfter(completed, cfg) {
  var every = Math.max(1, cfg.sessionsBeforeLongBreak)
  return completed > 0 && completed % every === 0 ? "long" : "short"
}

// 1-based position of the current (or next) focus session within its set.
function sessionInSet(completed, phase, cfg) {
  var every = Math.max(1, cfg.sessionsBeforeLongBreak)
  if (phase === "focus" || phase === "idle") return (completed % every) + 1
  // During a break the session that just ended is the one to show.
  var shown = completed % every
  return shown === 0 ? every : shown
}

function formatClock(ms) {
  var total = Math.max(0, Math.ceil(Number(ms || 0) / 1000))
  var hours = Math.floor(total / 3600)
  var minutes = Math.floor((total % 3600) / 60)
  var seconds = total % 60
  var mm = (minutes < 10 ? "0" : "") + minutes
  var ss = (seconds < 10 ? "0" : "") + seconds
  return hours > 0 ? hours + ":" + mm + ":" + ss : mm + ":" + ss
}

function matchTimerPreset(cfg) {
  for (var i = 0; i < TIMER_PRESETS.length; i++) {
    var p = TIMER_PRESETS[i]
    if (p.focus === cfg.focusMinutes && p.short === cfg.shortBreakMinutes
        && p.long === cfg.longBreakMinutes && p.sessions === cfg.sessionsBeforeLongBreak) return p.id
  }
  return "custom"
}

function timerPresetPatch(id) {
  for (var i = 0; i < TIMER_PRESETS.length; i++) {
    var p = TIMER_PRESETS[i]
    if (p.id === id) return {
      focusMinutes: p.focus,
      shortBreakMinutes: p.short,
      longBreakMinutes: p.long,
      sessionsBeforeLongBreak: p.sessions
    }
  }
  return null
}

// ---------------------------------------------------------------- sound

function soundPreset(id) {
  for (var i = 0; i < SOUND_PRESETS.length; i++) if (SOUND_PRESETS[i].id === id) return SOUND_PRESETS[i]
  return null
}

// Presets are matched on what they are (color, tone kind and frequencies),
// not on volume, so turning a preset up or down keeps its name.
function matchSoundPreset(cfg) {
  for (var i = 0; i < SOUND_PRESETS.length; i++) {
    var p = SOUND_PRESETS[i]
    if (p.noiseColor !== cfg.noiseColor || p.beatMode !== cfg.beatMode) continue
    if (p.beatMode === "off" || (p.beatHz === cfg.beatHz && p.carrierHz === cfg.carrierHz)) return p.id
  }
  return "custom"
}

function soundPresetPatch(id) {
  var p = soundPreset(id)
  if (!p) return null
  var patch = { noiseColor: p.noiseColor, noiseVolume: p.noiseVolume, beatMode: p.beatMode }
  if (p.beatMode !== "off") {
    patch.beatHz = p.beatHz
    patch.carrierHz = p.carrierHz
    patch.toneVolume = p.toneVolume
  }
  return patch
}

// EEG band names for a beat frequency (Hz).
function bandFor(hz) {
  if (hz < 4) return "delta"
  if (hz < 8) return "theta"
  if (hz < 13) return "alpha"
  if (hz < 30) return "beta"
  return "gamma"
}

// Square-law volume curve: half the slider is about -12 dB.
function gain(percent) {
  var v = Math.max(0, Math.min(100, Number(percent) || 0)) / 100
  return (v * v).toFixed(4)
}

function _fixed(n) {
  return Number(n).toFixed(3)
}

function toneExpressions(cfg) {
  var a = TONE_AMPLITUDE
  if (cfg.beatMode === "binaural") {
    // Each ear gets its own sine; the brainstem hears their difference.
    var left = cfg.carrierHz - cfg.beatHz / 2
    var right = cfg.carrierHz + cfg.beatHz / 2
    return a + "*sin(2*PI*" + _fixed(left) + "*t)|" + a + "*sin(2*PI*" + _fixed(right) + "*t)"
  }
  // Isochronic: the carrier pulsed by a raised cosine, so no clicks.
  var mono = a + "*sin(2*PI*" + _fixed(cfg.carrierHz) + "*t)*(0.5-0.5*cos(2*PI*" + _fixed(cfg.beatHz) + "*t))"
  return mono + "|" + mono
}

function _ffmpegBase() {
  return ["ffmpeg", "-nostdin", "-hide_banner", "-loglevel", "error"]
}

function _pulseOut(streamName) {
  // Each stream gets its own application name: WirePlumber remembers volume
  // per application, and the noise's volume must never carry over to alerts.
  return ["-f", "pulse", "-name", streamName, "-stream_name", streamName, streamName]
}

// Noise and tone are two separate PipeWire streams. Each generator runs at a
// fixed reference level and its volume is the stream's own volume, set live by
// the service, so moving a volume slider never restarts (or interrupts) the
// sound. Only a change in what is generated (color, beat, carrier) starts a new
// generator, which the service crossfades in over FADE_IN seconds.
var NOISE_STREAM = "Focus noise"
var TONE_STREAM = "Focus tone"
var ALERT_STREAM = "Focus alert"
var FADE_IN = 0.4
// Silent lead-in before the fade, so the stream's volume is set before it is heard.
var FADE_DELAY = 0.15

// Linear amplitude for a 0-100 slider, the same square-law curve as gain().
function linearGain(percent) {
  var v = Math.max(0, Math.min(100, Number(percent) || 0)) / 100
  return v * v
}

// Quickshell's PwNodeAudio.volume is perceptual: PipeWire receives its cube
// (measured: 0.25 -> channelVolumes 0.015625). Hand it the cube root so the
// stream's real amplitude is linearGain(percent).
function streamVolume(percent) {
  return Math.pow(linearGain(percent), 1 / 3)
}

// argv for the noise generator, or [] when there is no noise to play.
function noiseCommand(cfg) {
  if (cfg.noiseColor === "off" || cfg.noiseVolume <= 0 || NOISE_AMPLITUDE[cfg.noiseColor] === undefined) return []
  // Two independently seeded generators, one per ear, sound wider than one
  // copied to both, and keep the noise from correlating with a binaural tone.
  var src = "anoisesrc=color=" + cfg.noiseColor + ":amplitude=" + NOISE_AMPLITUDE[cfg.noiseColor] + ":sample_rate=" + SAMPLE_RATE
  return _ffmpegBase().concat([
    "-f", "lavfi", "-i", src + ":seed=11",
    "-f", "lavfi", "-i", src + ":seed=29",
    "-filter_complex", "[0:a][1:a]join=inputs=2:channel_layout=stereo,afade=t=in:st=" + FADE_DELAY + ":d=" + FADE_IN + ",alimiter=limit=0.9:level=0[out]",
    "-map", "[out]"
  ]).concat(_pulseOut(NOISE_STREAM))
}

// argv for the binaural / isochronic tone, or [] when there is none.
function toneCommand(cfg) {
  if (cfg.beatMode === "off" || cfg.toneVolume <= 0) return []
  return _ffmpegBase().concat([
    "-f", "lavfi", "-i", "aevalsrc=exprs='" + toneExpressions(cfg) + "':sample_rate=" + SAMPLE_RATE + ":channel_layout=stereo",
    "-af", "afade=t=in:st=" + FADE_DELAY + ":d=" + FADE_IN
  ]).concat(_pulseOut(TONE_STREAM))
}

function alertSound(id) {
  for (var i = 0; i < ALERT_SOUNDS.length; i++) if (ALERT_SOUNDS[i].id === id) return ALERT_SOUNDS[i]
  return null
}

function expandHome(path, home) {
  var p = String(path || "").trim()
  if (p === "~") return home
  if (p.indexOf("~/") === 0) return home + p.slice(1)
  return p
}

// argv to play an alert once, or [] for silence. A custom file is read only
// through ffmpeg's file: protocol (never a URL) and capped at 30 seconds.
function alertCommand(id, volumePercent, customPath, home) {
  var sound = alertSound(id)
  if (!sound || id === "none" || Number(volumePercent) <= 0) return []
  var args = _ffmpegBase()
  if (sound.expr) {
    args.push("-f", "lavfi", "-i", "aevalsrc=exprs='" + sound.expr + "':sample_rate=" + SAMPLE_RATE + ":duration=" + sound.duration)
  } else if (sound.file) {
    args.push("-i", "file:" + sound.file)
  } else {
    var path = expandHome(customPath, home)
    if (path === "" || path.charAt(0) !== "/") return []
    args.push("-i", "file:" + path, "-t", "30")
  }
  args.push("-af", "volume=" + gain(volumePercent))
  return args.concat(_pulseOut(ALERT_STREAM))
}

// ---------------------------------------------------------------- glyphs

var GLYPH = {
  timer: "󱎫",
  focus: "󰔛",
  coffee: "󰅶",
  pause: "󰏤",
  play: "󰐊",
  reset: "󰜉",
  skip: "󰒭",
  headphones: "󰋋",
  bell: "󰂞",
  tune: "󰘮",
  wave: "󱑽",
  left: "󰅁",
  right: "󰅂"
}

// `fresh` is a phase that is queued but has not started yet.
function barGlyph(phase, running, fresh) {
  if (phase === "idle") return GLYPH.timer
  if (!running && !fresh) return GLYPH.pause
  return isBreak(phase) ? GLYPH.coffee : GLYPH.focus
}

// Keep the long-running generator from outliving the shell even if the shell
// is killed outright: the kernel sends it SIGTERM when its parent dies.
function guarded(argv) {
  return argv.length === 0 ? [] : ["setpriv", "--pdeathsig", "TERM", "--"].concat(argv)
}
