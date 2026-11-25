.pragma library

// UI helpers only. Everything about timers, presets, sounds and settings
// belongs to the Focus engine (daemon/); the panel draws what it reports.

var PLUGIN_ID = "local.focus"

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
  sine: "󰭌",
  left: "󰅁",
  right: "󰅂",
  power: "󰐥",
  bellOff: "󰂛"
}

function isBreak(phase) {
  return phase === "short" || phase === "long"
}

// `fresh` is a phase that is queued but has not started yet.
function barGlyph(phase, running, fresh) {
  if (phase === "idle") return GLYPH.timer
  if (!running && !fresh) return GLYPH.pause
  return isBreak(phase) ? GLYPH.coffee : GLYPH.focus
}

// mm:ss, or h:mm:ss from an hour up; rounds up so 00:00 means done.
function formatClock(ms) {
  var total = Math.max(0, Math.ceil(Number(ms || 0) / 1000))
  var hours = Math.floor(total / 3600)
  var minutes = Math.floor((total % 3600) / 60)
  var seconds = total % 60
  var mm = (minutes < 10 ? "0" : "") + minutes
  var ss = (seconds < 10 ? "0" : "") + seconds
  return hours > 0 ? hours + ":" + mm + ":" + ss : mm + ":" + ss
}

// The EEG band of a beat frequency, from the engine's catalog
// ([[name, fromHz], ...] in ascending order).
function bandFor(hz, bands) {
  var name = ""
  for (var i = 0; bands && i < bands.length; i++) if (hz >= bands[i][1]) name = bands[i][0]
  return name
}

function find(list, id) {
  for (var i = 0; list && i < list.length; i++) if (list[i].id === id) return list[i]
  return null
}
