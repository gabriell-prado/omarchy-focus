import QtQuick
import Quickshell
import Quickshell.Io
import "Model.js" as Model

// Client for the Focus engine, the omarchy-focus daemon (daemon/ in this
// repository). One instance for the whole shell: it keeps a single
// subscription open on the engine's socket, mirrors the state it pushes, and
// sends every action the panels ask for. The engine owns all behaviour; see
// PROTOCOL.md for the contract.
Item {
  id: root

  property var shell: null
  property var manifest: null

  readonly property string socketPath: (Quickshell.env("XDG_RUNTIME_DIR") || "") + "/omarchy-focus.sock"

  // What the engine last said. `engine` is null while it is unreachable.
  property var catalog: null
  property var engine: null
  readonly property bool online: sock.connected && engine !== null

  // The countdown is derived locally from endsAt, so the engine never has
  // to push once a second.
  property real now: Date.now()

  readonly property var cfg: engine ? engine.settings : ({})
  readonly property string phase: engine ? engine.phase : "idle"
  readonly property bool running: !!(engine && engine.running)
  readonly property bool fresh: engine ? engine.fresh : true
  readonly property bool paused: !!(engine && engine.paused)
  // A session is in progress (running or paused), so its time can be changed.
  readonly property bool canAdjust: phase !== "idle" && !fresh
  readonly property real totalMs: engine ? engine.totalMs : 0
  readonly property real leftMs: !engine ? 0 : (engine.running ? Math.max(0, engine.endsAt - now) : engine.leftMs)
  readonly property real progress: !engine || phase === "idle" || totalMs <= 0 ? 0 : Math.max(0, Math.min(1, 1 - leftMs / totalMs))
  readonly property int doneInSet: engine ? engine.doneInSet : 0
  readonly property int todayCount: engine ? engine.today : 0
  readonly property string timerPreset: engine ? engine.timerPreset : "custom"
  readonly property string soundPreset: engine ? engine.soundPreset : "custom"
  readonly property bool soundPlaying: !!(engine && engine.sound.playing)
  readonly property bool soundWanted: !!(engine && engine.sound.wanted)
  readonly property bool soundFailed: !!(engine && engine.sound.failed)

  Socket {
    id: sock
    path: root.socketPath
    parser: SplitParser {
      splitMarker: "\n"
      onRead: function(line) { root.receive(line) }
    }
    onConnectedChanged: {
      if (connected) root.send({ cmd: "subscribe" })
      else root.engine = null
    }
  }

  // Connect at start, and again whenever the engine restarts or goes away.
  Timer {
    interval: 2000
    repeat: true
    triggeredOnStart: true
    running: !sock.connected
    onTriggered: sock.connected = true
  }

  Timer {
    interval: 500
    repeat: true
    running: root.running
    onTriggered: root.now = Date.now()
  }

  function receive(line) {
    var msg = null
    try { msg = JSON.parse(line) } catch (e) { return }
    if (!msg || typeof msg !== "object") return
    if (msg.event === "catalog") {
      catalog = msg.catalog
    } else if (msg.event === "state") {
      now = Date.now()
      engine = msg.state
    } else if (msg.ok === false) {
      console.warn("Focus engine: " + msg.error)
    }
  }

  function send(request) {
    if (!sock.connected) return false
    sock.write(JSON.stringify(request) + "\n")
    sock.flush()
    return true
  }

  // ---------------------------------------------------------------- actions

  function toggle() { send({ cmd: "toggle" }) }
  function start() { send({ cmd: "start" }) }
  function pause() { send({ cmd: "pause" }) }
  function reset() { send({ cmd: "reset" }) }
  function skip() { send({ cmd: "skip" }) }
  // Add (or, negative, take away) minutes from the session in progress.
  function adjust(minutes) { if (canAdjust) send({ cmd: "adjust", minutes: minutes }) }

  function update(patch) { send({ cmd: "set", patch: patch }) }
  function applyTimerPreset(id) { send({ cmd: "preset", kind: "timer", id: id }) }

  // Choosing or adjusting a sound means wanting to hear it: the engine
  // switches it on and, when it follows the timer, auditions it until the
  // panel closes (or this connection ends).
  function applySoundPreset(id) {
    send({ cmd: "preset", kind: "sound", id: id })
    audition()
  }
  function chooseSound(patch) {
    update(Object.assign({ soundEnabled: true }, patch || {}))
    audition()
  }
  function setSoundEnabled(on) {
    send({ cmd: "sound", on: on })
    if (on) audition()
  }
  function toggleSound() { send({ cmd: "sound" }) }
  function audition() { send({ cmd: "audition", on: true }) }
  function endAudition() { send({ cmd: "audition", on: false }) }

  // One alert at a time: the engine cuts off any alert still sounding.
  function previewAlert(id) { send({ cmd: "alert", id: id }) }

  function startEngine() {
    Quickshell.execDetached(["systemctl", "--user", "start", "omarchy-focus.service"])
  }
}
