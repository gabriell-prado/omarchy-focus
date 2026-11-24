// Run: node ~/.config/omarchy/plugins/focus/tests/model.test.mjs [--ffmpeg]
// --ffmpeg also renders every generated command to a null sink to prove the
// filter graphs parse (no audio is played).
import { readFileSync } from "node:fs"
import { execFileSync } from "node:child_process"
import { fileURLToPath } from "node:url"
import assert from "node:assert/strict"

const dir = fileURLToPath(new URL("..", import.meta.url))
const source = readFileSync(dir + "Model.js", "utf8").replace(/^\.pragma library\s*$/m, "")
const M = new Function(source + "\nreturn { PLUGIN_ID, DEFAULTS, SOUND_PRESETS, TIMER_PRESETS, ALERT_SOUNDS, normalizeSettings, compactSettings, findEntry, phaseMs, breakAfter, sessionInSet, formatClock, matchTimerPreset, timerPresetPatch, matchSoundPreset, soundPresetPatch, bandFor, gain, noiseCommand, alertCommand, expandHome, barGlyph, guarded, GLYPH, toneCommand, linearGain, streamVolume }")()

let failures = 0
function test(name, fn) {
  try { fn(); console.log("ok   " + name) }
  catch (e) { failures++; console.log("FAIL " + name + "\n     " + e.message) }
}

const D = M.normalizeSettings({})

test("defaults normalize to themselves", () => {
  assert.deepEqual(M.normalizeSettings(M.DEFAULTS), M.DEFAULTS)
  assert.deepEqual(M.compactSettings(D), {})
})

test("invalid values fall back or clamp", () => {
  const s = M.normalizeSettings({ focusMinutes: "999", shortBreakMinutes: -3, noiseColor: "purple",
    beatMode: 4, soundEnabled: "yes", alertVolume: null, focusEndSound: "nope", customSoundPath: 7, extra: 1 })
  assert.equal(s.focusMinutes, 180)
  assert.equal(s.shortBreakMinutes, 1)
  assert.equal(s.noiseColor, "pink")
  assert.equal(s.beatMode, "off")
  assert.equal(s.soundEnabled, false)
  assert.equal(s.alertVolume, 70)
  assert.equal(s.focusEndSound, "bowl")
  assert.equal(s.customSoundPath, "")
  assert.equal("extra" in s, false)
})

test("compact keeps only changes", () => {
  assert.deepEqual(M.compactSettings({ ...D, focusMinutes: 50, id: "x" }), { focusMinutes: 50 })
})

test("findEntry scans every section", () => {
  const cfg = { layout: { left: [{ id: "a" }], tray: [{ id: "local.focus", focusMinutes: 30 }] } }
  assert.equal(M.findEntry(cfg, "local.focus").focusMinutes, 30)
  assert.equal(M.findEntry(cfg, "missing"), null)
  assert.equal(M.findEntry(null, "x"), null)
})

test("long break every N sessions", () => {
  const s = { ...D, sessionsBeforeLongBreak: 4 }
  assert.deepEqual([1, 2, 3, 4, 5, 8].map(n => M.breakAfter(n, s)), ["short", "short", "short", "long", "short", "long"])
  assert.equal(M.breakAfter(1, { ...D, sessionsBeforeLongBreak: 1 }), "long")
})

test("session position in set", () => {
  const s = { ...D, sessionsBeforeLongBreak: 4 }
  assert.equal(M.sessionInSet(0, "idle", s), 1)
  assert.equal(M.sessionInSet(1, "focus", s), 2)
  assert.equal(M.sessionInSet(1, "short", s), 1)
  assert.equal(M.sessionInSet(4, "long", s), 4)
})

test("clock formatting rounds up", () => {
  assert.equal(M.formatClock(25 * 60000), "25:00")
  assert.equal(M.formatClock(59001), "01:00")
  assert.equal(M.formatClock(999), "00:01")
  assert.equal(M.formatClock(0), "00:00")
  assert.equal(M.formatClock(-5), "00:00")
  assert.equal(M.formatClock(90 * 60000), "1:30:00")
})

test("timer presets round-trip", () => {
  for (const p of M.TIMER_PRESETS) {
    assert.equal(M.matchTimerPreset(M.normalizeSettings(M.timerPresetPatch(p.id))), p.id)
  }
  assert.equal(M.matchTimerPreset({ ...D, focusMinutes: 33 }), "custom")
})

test("sound presets round-trip and are distinct", () => {
  const seen = new Set()
  for (const p of M.SOUND_PRESETS) {
    const s = M.normalizeSettings({ ...D, ...M.soundPresetPatch(p.id) })
    assert.equal(M.matchSoundPreset(s), p.id)
    assert.equal(M.matchSoundPreset({ ...s, noiseVolume: 3 }), p.id, "volume must not change the preset")
    const key = [p.noiseColor, p.beatMode, p.beatHz, p.carrierHz].join("/")
    assert.equal(seen.has(key), false, "duplicate preset " + p.id)
    seen.add(key)
  }
})

test("bands", () => {
  assert.deepEqual([2, 6, 10, 16, 40].map(M.bandFor), ["delta", "theta", "alpha", "beta", "gamma"])
})

test("noise command shapes", () => {
  assert.deepEqual(M.noiseCommand({ ...D, noiseColor: "off" }), [])
  assert.deepEqual(M.noiseCommand({ ...D, noiseVolume: 0 }), [])
  const pink = M.noiseCommand(D)
  assert.equal(pink[0], "ffmpeg")
  assert.ok(pink.join(" ").includes("color=pink"))
  assert.equal(pink.join(" ").includes("aevalsrc"), false)
  assert.equal(pink.at(-1), "Focus noise")
  assert.equal(pink[pink.indexOf("-name") + 1], "Focus noise", "own application name")
})

test("tone command shapes", () => {
  assert.deepEqual(M.toneCommand(D), [], "beat off")
  assert.deepEqual(M.toneCommand({ ...D, beatMode: "binaural", toneVolume: 0 }), [])
  const bin = M.toneCommand({ ...D, beatMode: "binaural", beatHz: 16, carrierHz: 250 })
  assert.ok(bin.join(" ").includes("242.000*t)|"), "left ear carrier - beat/2")
  assert.ok(bin.join(" ").includes("258.000*t)"), "right ear carrier + beat/2")
  assert.equal(bin.at(-1), "Focus tone")
  assert.ok(M.toneCommand({ ...D, beatMode: "isochronic", beatHz: 40 }).join(" ").includes("cos(2*PI*40.000*t)"))
})

test("volume never changes a generator (no restart, no gap)", () => {
  assert.deepEqual(M.noiseCommand({ ...D, noiseVolume: 5 }), M.noiseCommand({ ...D, noiseVolume: 100 }))
  const t = { ...D, beatMode: "binaural" }
  assert.deepEqual(M.toneCommand({ ...t, toneVolume: 5 }), M.toneCommand({ ...t, toneVolume: 100 }))
  assert.equal(M.linearGain(50), 0.25)
  assert.equal(M.linearGain(0), 0)
  assert.equal(M.linearGain(150), 1)
  assert.ok(Math.abs(Math.pow(M.streamVolume(50), 3) - 0.25) < 1e-12, "cube of the stream volume is the linear gain")
})

test("alert commands", () => {
  assert.deepEqual(M.alertCommand("none", 70, "", "/home/u"), [])
  assert.deepEqual(M.alertCommand("chime", 0, "", "/home/u"), [])
  assert.deepEqual(M.alertCommand("custom", 70, "", "/home/u"), [])
  assert.deepEqual(M.alertCommand("custom", 70, "relative.ogg", "/home/u"), [])
  assert.deepEqual(M.alertCommand("custom", 70, "https://x/y.mp3", "/home/u"), [])
  const custom = M.alertCommand("custom", 70, "~/a b.ogg", "/home/u")
  assert.ok(custom.includes("file:/home/u/a b.ogg"))
  assert.ok(M.alertCommand("bell", 50, "", "/h").includes("file:/usr/share/sounds/freedesktop/stereo/bell.oga"))
})

test("glyphs", () => {
  assert.notEqual(M.barGlyph("idle", false, true), M.barGlyph("focus", true, false))
  assert.notEqual(M.barGlyph("focus", true, false), M.barGlyph("short", true, false))
  assert.equal(M.barGlyph("short", false, false), M.barGlyph("focus", false, false), "paused looks the same in any phase")
  assert.equal(M.barGlyph("short", false, true), M.barGlyph("short", true, false), "a queued break shows the break glyph")
})

test("guarded command", () => {
  assert.deepEqual(M.guarded([]), [])
  assert.deepEqual(M.guarded(["ffmpeg"]).slice(0, 4), ["setpriv", "--pdeathsig", "TERM", "--"])
})

if (process.argv.includes("--ffmpeg")) {
  const toNull = (argv, seconds) => {
    const i = argv.indexOf("-f", argv.indexOf("-map") > 0 ? argv.indexOf("-map") : argv.indexOf("-af"))
    const head = argv.slice(1, i)
    execFileSync("ffmpeg", [...head, "-t", String(seconds), "-f", "null", "-"], { stdio: ["ignore", "ignore", "pipe"] })
  }
  for (const p of M.SOUND_PRESETS) {
    const s = M.normalizeSettings({ ...D, ...M.soundPresetPatch(p.id) })
    test("ffmpeg renders preset " + p.id, () => {
      toNull(M.noiseCommand(s), 2)
      if (M.toneCommand(s).length) toNull(M.toneCommand(s), 2)
    })
  }
  for (const s of M.ALERT_SOUNDS) {
    const argv = M.alertCommand(s.id, 70, "/usr/share/sounds/freedesktop/stereo/bell.oga", "/h")
    if (argv.length) test("ffmpeg renders alert " + s.id, () => toNull(argv, 8))
  }
}

if (failures) { console.log(`\n${failures} failing`); process.exit(1) }
console.log("\nall passed")
