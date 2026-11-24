import QtQuick
import Quickshell
import Quickshell.Io
import Quickshell.Services.Pipewire
import qs.Commons
import qs.Commons as Commons
import "Model.js" as Model

// The one Focus engine for the whole shell. The bar widget is built once per
// monitor, so the timer, the sound generator, alerts and IPC live here, and
// every widget just draws this state and calls into it.
//
// Settings live in this plugin's bar entry in ~/.config/omarchy/shell.json.
// Timer progress lives in $XDG_STATE_HOME/omarchy-focus/state.json, so a shell
// restart or plugin reload resumes a running session instead of losing it.
Item {
  id: root

  property var shell: null
  property var manifest: null

  Commons.Translations { id: strings; directory: Qt.resolvedUrl("i18n") }

  // ---------------------------------------------------------------- settings

  // What the widgets last saw in shell.json (or what we just wrote there).
  property var pushedSettings: null
  readonly property var configEntry: Model.findEntry(shell ? shell.barConfig : null, Model.PLUGIN_ID)
  readonly property bool settingsKnown: pushedSettings !== null || configEntry !== null
  readonly property var cfg: Model.normalizeSettings(pushedSettings !== null ? pushedSettings : (configEntry || {}))

  function noteSettings(raw) {
    var next = Model.compactSettings(raw || {})
    if (pushedSettings !== null && JSON.stringify(next) === JSON.stringify(pushedSettings)) return
    pushedSettings = next
  }

  function update(patch) {
    var next = Model.compactSettings(Object.assign({}, cfg, patch || {}))
    pushedSettings = next
    if (shell && typeof shell.updateEntryInline === "function")
      shell.updateEntryInline(Model.PLUGIN_ID, next)
  }

  // ---------------------------------------------------------------- timer

  property string phase: "idle"      // idle | focus | short | long
  property bool running: false
  // A queued phase that has not been started: its length follows the settings.
  property bool fresh: true
  property real endsAt: 0
  property real remainingMs: 0
  property real phaseTotalMs: 0
  property int completedFocus: 0     // in the current set, reset by a long break
  property int completedToday: 0
  property string day: ""
  property real now: Date.now()

  readonly property bool paused: phase !== "idle" && !running && !fresh
  readonly property real durationMs: Model.phaseMs(phase === "idle" ? "focus" : phase, cfg)
  readonly property real leftMs: running ? Math.max(0, endsAt - now) : (fresh ? durationMs : remainingMs)
  readonly property real totalMs: !fresh && phaseTotalMs > 0 ? phaseTotalMs : durationMs
  readonly property real progress: phase === "idle" || totalMs <= 0 ? 0 : Math.max(0, Math.min(1, 1 - leftMs / totalMs))
  readonly property int sessionNumber: Model.sessionInSet(completedFocus, phase, cfg)
  readonly property int doneInSet: phase === "long" ? cfg.sessionsBeforeLongBreak : completedFocus % Math.max(1, cfg.sessionsBeforeLongBreak)
  readonly property string todayKey: dayKey(now)
  readonly property int todayCount: day === todayKey ? completedToday : 0

  function dayKey(ms) {
    var d = new Date(ms)
    return d.getFullYear() + "-" + (d.getMonth() + 1) + "-" + d.getDate()
  }

  function start() {
    if (phase === "idle") preparePhase("focus")
    if (running) return
    var length = fresh ? Model.phaseMs(phase, cfg) : remainingMs
    if (fresh) phaseTotalMs = length
    now = Date.now()
    endsAt = now + length
    fresh = false
    running = true
    save()
  }

  function pause() {
    if (!running) return
    now = Date.now()
    remainingMs = Math.max(0, endsAt - now)
    running = false
    save()
  }

  function toggle() {
    if (running) pause()
    else start()
  }

  function reset() {
    running = false
    phase = "idle"
    fresh = true
    remainingMs = 0
    phaseTotalMs = 0
    completedFocus = 0
    save()
  }

  function skip() {
    if (phase !== "idle") finishPhase(true, false)
  }

  function preparePhase(next) {
    phase = next
    running = false
    fresh = true
    remainingMs = 0
    phaseTotalMs = Model.phaseMs(next, cfg)
  }

  function countToday() {
    var key = dayKey(Date.now())
    if (day !== key) {
      day = key
      completedToday = 0
    }
    completedToday += 1
  }

  // skipped: the user moved on early (no alert, not counted as done today).
  // quiet: it ended while the shell was not running (no alert, no auto-start).
  function finishPhase(skipped, quiet) {
    var ended = phase
    var next
    if (ended === "focus") {
      completedFocus += 1
      if (!skipped) countToday()
      next = Model.breakAfter(completedFocus, cfg)
    } else {
      if (ended === "long") completedFocus = 0
      next = "focus"
    }
    var autoStart = !quiet && (next === "focus" ? cfg.autoStartFocus : cfg.autoStartBreaks)
    if (!skipped && !quiet) announce(ended, next, autoStart)
    preparePhase(next)
    if (autoStart) start()
    else save()
  }

  function tick() {
    now = Date.now()
    if (running && now >= endsAt) finishPhase(false, false)
  }

  Timer {
    interval: 500
    repeat: true
    running: root.running
    onTriggered: root.tick()
  }

  // Keeps "today" honest across midnight while nothing is running.
  Timer {
    interval: 60000
    repeat: true
    running: !root.running
    onTriggered: root.now = Date.now()
  }

  // ---------------------------------------------------------------- alerts

  // One alert at a time: a new alert or preview stops the one playing.
  property var _pendingAlert: []

  Process {
    id: alertProc
    onExited: Qt.callLater(root._startPendingAlert)
  }

  function _startPendingAlert() {
    if (alertProc.running || _pendingAlert.length === 0) return
    alertProc.command = _pendingAlert
    _pendingAlert = []
    alertProc.running = true
  }

  function playAlert(id) {
    _pendingAlert = Model.guarded(Model.alertCommand(id, cfg.alertVolume, cfg.customSoundPath, Quickshell.env("HOME") || ""))
    if (alertProc.running) alertProc.signal(9)  // cut it off now; the next one starts once it is gone
    else _startPendingAlert()
  }

  function previewAlert(id) {
    playAlert(id)
  }

  function stopAlert() {
    _pendingAlert = []
    if (alertProc.running) alertProc.signal(9)
  }

  function announce(ended, next, autoStart) {
    playAlert(ended === "focus" ? cfg.focusEndSound : cfg.breakEndSound)
    if (!cfg.desktopNotifications) return
    var minutes = Model.phaseMinutes(next, cfg)
    var title = strings.t(ended === "focus" ? "notify.focusDone.title" : "notify.breakDone.title")
    var body = strings.tn("notify." + next + "." + (autoStart ? "auto" : "manual"), minutes)
      .arg(Commons.I18n.formatNumber(minutes))
    Quickshell.execDetached(["notify-send", "--app-name=Focus",
      "--hint=string:x-canonical-private-synchronous:" + Model.PLUGIN_ID, "--", title, body])
  }

  // ---------------------------------------------------------------- sound
  //
  // The sound is continuous: it plays for as long as it is switched on, and
  // only follows the timer when "Only during focus sessions" is set. In that
  // mode, changing the sound from the open panel auditions it until the panel
  // closes, so a choice is heard even while no focus session is running.

  property bool auditioning: false
  function audition() { auditioning = true }
  function endAudition() { auditioning = false }

  readonly property bool soundReady: cfg.soundEnabled && settingsKnown && restored
  readonly property bool soundWanted: soundReady && (!cfg.soundOnlyDuringFocus || (phase === "focus" && running))
  readonly property bool soundActive: soundWanted || (soundReady && auditioning)
  readonly property bool soundPlaying: noiseChannel.playing || toneChannel.playing
  readonly property bool soundFailed: noiseChannel.failed || toneChannel.failed

  // Noise and tone each run in their own generator and PipeWire stream. The
  // command depends only on what is generated, never on volume.
  Channel {
    id: noiseChannel
    label: "noise"
    argv: root.soundActive ? Model.guarded(Model.noiseCommand(root.cfg)) : []
  }
  Channel {
    id: toneChannel
    label: "tone"
    argv: root.soundActive ? Model.guarded(Model.toneCommand(root.cfg)) : []
  }

  // Volume is applied live to the generators' streams, so moving a slider
  // changes the level without restarting anything.
  readonly property real noiseLevel: Model.streamVolume(cfg.noiseVolume)
  readonly property real toneLevel: Model.streamVolume(cfg.toneVolume)

  readonly property var ownStreams: {
    var out = []
    var nodes = Pipewire.nodes ? Pipewire.nodes.values : []
    for (var i = 0; i < nodes.length; i++) {
      var n = nodes[i]
      if (n && n.isStream && (n.name === Model.NOISE_STREAM || n.name === Model.TONE_STREAM)) out.push(n)
    }
    return out
  }

  PwObjectTracker { objects: root.ownStreams }

  Repeater {
    model: root.ownStreams
    delegate: Item {
      id: streamVolume
      required property var modelData
      readonly property var audio: modelData && modelData.ready ? modelData.audio : null
      readonly property real target: modelData && modelData.name === Model.TONE_STREAM ? root.toneLevel : root.noiseLevel
      // A new stream reports 0, then WirePlumber's remembered volume, before
      // it settles; keep putting the target back until it has.
      property bool settling: true

      function apply() {
        if (audio) audio.volume = target
      }

      onAudioChanged: {
        if (!audio) return
        settling = true
        settleTimer.restart()
        apply()
      }
      onTargetChanged: apply()
      Component.onCompleted: apply()

      Connections {
        target: streamVolume.audio
        ignoreUnknownSignals: true
        function onVolumesChanged() {
          if (streamVolume.settling && Math.abs(streamVolume.audio.volume - streamVolume.target) > 0.000001) streamVolume.apply()
        }
      }

      Timer {
        id: settleTimer
        interval: 1500
        onTriggered: streamVolume.settling = false
      }
    }
  }

  // One generator slot. A new command crossfades in over the old one, which
  // is stopped once the new one has faded in; an empty command stops at once.
  // A generator that dies on its own is restarted, up to 5 quick failures.
  // Generators and alerts are stopped with SIGKILL: on SIGTERM ffmpeg first
  // plays out its ~0.5 s audio buffer, which would overlap the next sound.
  component Channel: Item {
    id: channel
    property string label: ""
    property var argv: []
    readonly property string key: JSON.stringify(argv)
    property var current: null
    property var retiring: []
    property int failures: 0
    property real startedAt: 0
    property bool failed: false
    property bool currentRunning: false
    readonly property bool playing: current !== null && currentRunning

    onKeyChanged: swap()
    Component.onCompleted: swap()

    function swap() {
      failures = 0
      failed = false
      retryTimer.stop()
      var old = current
      current = null
      currentRunning = false
      if (old) {
        retiring = retiring.concat([old])
        if (argv.length === 0) retireNow()
        else retireTimer.restart()
      }
      launch()
    }

    function launch() {
      if (argv.length === 0 || current !== null) return
      var p = processComponent.createObject(channel, { command: argv })
      if (!p) return
      p.exited.connect(function(code) { channel.processExited(p, code) })
      p.runningChanged.connect(function() { if (p === channel.current) channel.currentRunning = p.running })
      current = p
      startedAt = Date.now()
      p.running = true
    }

    function retireNow() {
      retireTimer.stop()
      var list = retiring
      retiring = []
      for (var i = 0; i < list.length; i++) {
        list[i].retired = true
        if (list[i].running) list[i].signal(9)
        else list[i].destroy()
      }
    }

    function processExited(p, code) {
      if (p !== current) {
        p.destroy()
        return
      }
      current = null
      currentRunning = false
      p.destroy()
      if (argv.length === 0) return
      if (Date.now() - startedAt > 30000) failures = 0
      failures += 1
      if (failures <= 5) retryTimer.restart()
      else {
        failed = true
        console.warn("Focus: " + label + " generator keeps exiting (code " + code + "); giving up until the sound changes")
      }
    }

    function stopAll() {
      argv = []
      retireNow()
      if (current) {
        current.retired = true
        if (current.running) current.signal(9)
      }
    }

    Component.onDestruction: stopAll()

    Timer {
      id: retireTimer
      interval: (Model.FADE_DELAY + Model.FADE_IN) * 1000 + 200
      onTriggered: channel.retireNow()
    }

    Timer {
      id: retryTimer
      interval: 3000
      onTriggered: channel.launch()
    }

    Component {
      id: processComponent
      Process { property bool retired: false }
    }
  }

  Component.onDestruction: {
    noiseChannel.stopAll()
    toneChannel.stopAll()
    _pendingAlert = []
    if (alertProc.running) alertProc.signal(9)
  }

  // ---------------------------------------------------------------- state file

  readonly property string stateDir: (Quickshell.env("XDG_STATE_HOME") || ((Quickshell.env("HOME") || "") + "/.local/state")) + "/omarchy-focus"
  property bool restored: false

  FileView {
    id: stateFile
    path: root.stateDir + "/state.json"
    printErrors: false
    onLoaded: root.restore(text())
    onLoadFailed: root.restored = true
  }

  Component.onCompleted: Quickshell.execDetached(["mkdir", "-p", root.stateDir])

  function save() {
    if (!restored) return
    stateFile.setText(JSON.stringify({
      version: 1,
      phase: phase,
      running: running,
      fresh: fresh,
      endsAt: endsAt,
      remainingMs: remainingMs,
      phaseTotalMs: phaseTotalMs,
      completedFocus: completedFocus,
      completedToday: completedToday,
      day: day
    }) + "\n")
  }

  function _num(value, fallback) {
    var n = Number(value)
    return isFinite(n) && n >= 0 ? n : fallback
  }

  function restore(raw) {
    if (restored) return
    var s = null
    try { s = JSON.parse(raw) } catch (e) { s = null }
    if (s && typeof s === "object" && ["idle", "focus", "short", "long"].indexOf(s.phase) !== -1) {
      phase = s.phase
      fresh = s.fresh !== false
      remainingMs = Math.min(_num(s.remainingMs, 0), 24 * 3600000)
      phaseTotalMs = Math.min(_num(s.phaseTotalMs, 0), 24 * 3600000)
      completedFocus = Math.min(Math.round(_num(s.completedFocus, 0)), 1000)
      completedToday = Math.min(Math.round(_num(s.completedToday, 0)), 1000)
      day = typeof s.day === "string" ? s.day.slice(0, 16) : ""
      var ends = _num(s.endsAt, 0)
      if (s.running === true && phase !== "idle" && ends > 0) {
        var late = Date.now() - ends
        endsAt = ends
        running = true
        now = Date.now()
        restored = true
        // Ended long ago (shell was down, machine asleep): move on silently.
        if (late > 60000) finishPhase(false, true)
        // Otherwise the ticker finishes or continues it normally.
        return
      }
    }
    restored = true
  }

  // ---------------------------------------------------------------- IPC
  // omarchy-shell focus toggle | start | pause | reset | skip | sound | status | alert <sound>

  function statusJson() {
    return JSON.stringify({
      phase: phase,
      running: running,
      paused: paused,
      left: Model.formatClock(leftMs),
      leftMs: Math.round(leftMs),
      session: sessionNumber,
      sessions: cfg.sessionsBeforeLongBreak,
      today: todayCount,
      sound: soundPlaying
    })
  }

  ShellIpc {
    target: "focus"

    function toggle(): void { root.toggle() }
    function start(): void { root.start() }
    function pause(): void { root.pause() }
    function reset(): void { root.reset() }
    function skip(): void { root.skip() }
    function sound(): void { root.update({ soundEnabled: !root.cfg.soundEnabled }) }
    function alert(id: string): void { root.playAlert(id) }
    function status(): string { return root.statusJson() }
  }
}
