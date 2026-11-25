import QtQuick
import Quickshell
import qs.Commons
import qs.Commons as Commons
import qs.Ui
import "Model.js" as Model
import "ui"

// Bar widget and panel. One of these exists per monitor; all of them draw
// the single Service (the engine client) and send every action through it.
//
// Bar:   left click opens the panel, right click starts/pauses, middle click
//        skips to the next phase, scrolling adds or removes a minute from the
//        session in progress.
// Panel: Space/Enter start/pause, r reset, s skip, n sound, + / − one minute,
//        1-3 or ←/→ pages.
Panel {
  id: root
  moduleName: Model.PLUGIN_ID
  ipcTarget: Model.PLUGIN_ID

  Commons.Translations { id: strings; directory: Qt.resolvedUrl("i18n") }

  // ---------------------------------------------------------------- service

  property var svc: null

  function findService() {
    if (svc || !bar || !bar.shell || typeof bar.shell.serviceFor !== "function") return
    var found = bar.shell.serviceFor(Model.PLUGIN_ID)
    if (found) svc = found
  }

  // The service can register after the widget, and is rebuilt on plugin reloads.
  Timer {
    interval: 300
    repeat: true
    running: root.svc === null
    triggeredOnStart: true
    onTriggered: root.findService()
  }

  Context {
    id: uiCtx
    svc: root.svc
    strings: strings
    bar: root.bar
  }

  readonly property bool online: !!(svc && svc.online)
  readonly property var cfg: uiCtx.cfg
  readonly property string phase: svc ? svc.phase : "idle"
  readonly property bool running: !!(svc && svc.running)
  readonly property bool paused: !!(svc && svc.paused)
  readonly property bool fresh: svc ? svc.fresh : true
  readonly property real progress: svc ? svc.progress : 0
  readonly property int doneInSet: svc ? svc.doneInSet : 0
  readonly property int todayCount: svc ? svc.todayCount : 0
  readonly property string clock: uiCtx.digits(Model.formatClock(svc ? svc.leftMs : 0))

  readonly property string phaseName: uiCtx.t("phase." + phase)
  readonly property string phaseLine: {
    if (phase === "idle") return uiCtx.t("phase.idle")
    if (paused) return uiCtx.t("hero.paused").arg(phaseName)
    if (!running) return uiCtx.t("hero.upNext").arg(phaseName)
    return phaseName
  }
  readonly property string tooltip: {
    if (!online) return uiCtx.t("bar.tooltip.offline")
    if (phase === "idle") return uiCtx.t("bar.tooltip.idle")
    return uiCtx.t(running ? "bar.tooltip.running" : "bar.tooltip.paused").arg(phaseName).arg(clock)
  }

  // The QML-only version kept settings in this widget's shell.json entry. The
  // engine imports them on its first start; once it is up, clear them so
  // shell.json does not keep stale copies.
  function clearLegacySettings() {
    if (!online || !settings || !bar || !bar.shell || typeof bar.shell.updateEntryInline !== "function") return
    if (Object.keys(settings).some(function(k) { return k !== "id" }))
      bar.shell.updateEntryInline(Model.PLUGIN_ID, {})
  }
  onSettingsChanged: clearLegacySettings()
  onOnlineChanged: clearLegacySettings()

  // An audition lasts while the panel is open.
  onOpenedChanged: if (!opened && svc) svc.endAudition()

  // ---------------------------------------------------------------- pages

  property string page: "timer"
  readonly property var pages: ["timer", "noise", "alerts"]
  function cyclePage(delta) {
    var i = pages.indexOf(page)
    page = pages[(i + delta + pages.length) % pages.length]
  }

  // ---------------------------------------------------------------- bar button

  readonly property color barFg: bar ? bar.barForeground : Commons.Color.foreground
  // What the bar shows: "none" (icon), "time" (icon and time left) or
  // "ring" (a ring in the theme's accent that fills as the session passes).
  // The ring is only for a session that is running: idle, queued or paused,
  // the bar shows the same icon as in "none" (a queued break is a grey cup).
  readonly property string barDisplay: cfg.barDisplay || "time"
  readonly property bool showTime: online && barDisplay === "time" && phase !== "idle" && !(bar && bar.vertical)
  readonly property bool showRing: online && barDisplay === "ring" && running

  implicitWidth: button.implicitWidth
  implicitHeight: button.implicitHeight

  TextMetrics {
    id: clockMetrics
    font.family: uiCtx.fontFamily
    font.pixelSize: Style.bar.iconFont
    text: root.clock
  }

  BarIconButton {
    id: button
    anchors.fill: parent
    bar: root.bar
    text: Model.barGlyph(root.phase, root.running, root.fresh)
    iconComponent: root.showRing ? ringIcon : null
    labelText: root.showTime ? root.clock : ""
    // The status slot, like Elsewhen and Weather beside the clock; the time,
    // when shown, adds its own width plus BarIconButton's label-to-icon gap.
    slotSize: Style.bar.statusSlot + (root.showTime ? Math.ceil(clockMetrics.advanceWidth) + Style.space(4) : 0)
    // Grey only for a phase that is queued but not started (e.g. a break
    // that is due) or when the engine is unreachable; running, paused and
    // idle use the bar's own color.
    readonly property bool waiting: root.phase !== "idle" && root.fresh && !root.running
    foreground: root.online && !waiting ? root.barFg : Qt.darker(root.barFg, 1.55)
    tooltipText: root.opened ? "" : root.tooltip
    // Each mouse-wheel notch (120) is a minute; touchpads send smaller steps,
    // which add up to one.
    property real wheelAccumulated: 0
    onWheelMoved: function(delta) {
      if (!root.svc || !root.svc.canAdjust) return
      wheelAccumulated += delta
      while (Math.abs(wheelAccumulated) >= 120) {
        var step = wheelAccumulated > 0 ? 1 : -1
        wheelAccumulated -= step * 120
        root.svc.adjust(step)
      }
    }
    onPressed: function(b) {
      if (b === Qt.RightButton) {
        if (root.svc) root.svc.toggle()
      } else if (b === Qt.MiddleButton) {
        if (root.svc) root.svc.skip()
      } else {
        root.toggle()
      }
    }
  }

  // The ring: a faint full circle, and over it an arc in the theme's accent
  // from twelve o'clock, clockwise, as far as the running session has gone.
  // Only the border is drawn.
  Component {
    id: ringIcon

    Canvas {
      id: ring
      width: Style.bar.iconSize
      height: Style.bar.iconSize

      readonly property real progress: root.progress
      readonly property color track: Qt.rgba(root.barFg.r, root.barFg.g, root.barFg.b, 0.35)
      readonly property color fill: Commons.Color.accent

      onProgressChanged: requestPaint()
      onTrackChanged: requestPaint()
      onFillChanged: requestPaint()
      onWidthChanged: requestPaint()

      onPaint: {
        var ctx = getContext("2d")
        ctx.reset()
        var line = Math.max(1.5, width * 0.16)
        var radius = (Math.min(width, height) - line) / 2
        var cx = width / 2
        var cy = height / 2
        ctx.lineWidth = line
        ctx.strokeStyle = track
        ctx.beginPath()
        ctx.arc(cx, cy, radius, 0, 2 * Math.PI)
        ctx.stroke()
        if (progress > 0) {
          var start = -Math.PI / 2
          ctx.strokeStyle = fill
          ctx.lineCap = "round"
          ctx.beginPath()
          ctx.arc(cx, cy, radius, start, start + progress * 2 * Math.PI, false)
          ctx.stroke()
        }
      }
    }
  }

  // ---------------------------------------------------------------- panel

  KeyboardPanel {
    id: panel
    anchorItem: button
    owner: root
    bar: root.bar
    open: root.opened
    focusTarget: panel.loadedContent
    contentWidth: panel.fittedContentWidth(Style.space(460))
    contentHeight: panel.fittedContentHeight(panel.loadedContentHeight)

    content: PanelKeyCatcher {
      id: keys
      anchors.fill: parent
      implicitHeight: column.implicitHeight
      blocked: alertsPage.editing

      onActivateRequested: if (root.svc) root.svc.toggle()
      onCloseRequested: root.close()
      onTabRequested: function(direction) { root.switchPanel(direction) }
      onMoveRequested: function(dx, dy) {
        if (dx !== 0) root.cyclePage(dx)
        else flick.contentY = Math.max(0, Math.min(flick.contentHeight - flick.height, flick.contentY + dy * Style.space(48)))
      }
      onTextKey: function(text) {
        if (!root.svc) return
        if (text === "r") root.svc.reset()
        else if (text === "s") root.svc.skip()
        else if (text === "n") root.svc.setSoundEnabled(!root.cfg.soundEnabled)
        else if (text === "+" || text === "=") root.svc.adjust(1)
        else if (text === "-" || text === "_") root.svc.adjust(-1)
        else if (text >= "1" && text <= String(root.pages.length)) root.page = root.pages[Number(text) - 1]
      }

      Flickable {
        id: flick
        anchors.fill: parent
        contentWidth: width
        contentHeight: column.implicitHeight
        clip: true
        interactive: contentHeight > height + 1
        boundsBehavior: Flickable.StopAtBounds
        flickableDirection: Flickable.VerticalFlick

        Column {
          id: column
          width: flick.width
          spacing: Style.space(12)

          // ---------- Engine not reachable ----------
          Column {
            visible: !root.online
            width: parent.width
            spacing: Style.space(10)

            Text {
              width: parent.width
              textFormat: Text.PlainText
              text: uiCtx.t("engine.offline.title")
              color: uiCtx.fg
              font.family: uiCtx.fontFamily
              font.pixelSize: Style.font.title
              font.bold: true
              wrapMode: Text.WordWrap
            }
            Note { ctx: uiCtx; text: uiCtx.t("engine.offline.body"); opacity: 0.85 }
            Text {
              width: parent.width
              textFormat: Text.PlainText
              text: "systemctl --user enable --now omarchy-focus"
              color: uiCtx.fg
              font.family: uiCtx.fontFamily
              font.pixelSize: Style.font.bodySmall
              wrapMode: Text.WrapAnywhere
            }
            PanelButton {
              ctx: uiCtx
              width: parent.width
              iconText: Model.GLYPH.power
              text: uiCtx.t("engine.offline.start")
              onClicked: if (root.svc) root.svc.startEngine()
            }
          }

          // ---------- Hero: clock · phase · session dots ----------
          Item {
            visible: root.online
            width: parent.width
            implicitHeight: Math.max(heroClock.implicitHeight, heroSide.implicitHeight)

            Text {
              id: heroClock
              anchors.left: parent.left
              anchors.verticalCenter: parent.verticalCenter
              textFormat: Text.PlainText
              text: root.clock
              color: uiCtx.fg
              opacity: root.running || root.phase === "idle" ? 1 : 0.6
              font.family: uiCtx.fontFamily
              font.pixelSize: Math.round(Style.font.displayLarge * 1.6)
              font.bold: true
              Behavior on opacity { NumberAnimation { duration: Style.duration(160); easing.type: Easing.OutCubic } }
            }

            Column {
              id: heroSide
              anchors.left: heroClock.right
              anchors.leftMargin: Style.space(12)
              anchors.right: parent.right
              anchors.verticalCenter: parent.verticalCenter
              spacing: Style.space(6)

              Text {
                width: parent.width
                horizontalAlignment: Text.AlignRight
                textFormat: Text.PlainText
                text: root.phaseLine.toUpperCase()
                color: uiCtx.fg
                font.family: uiCtx.fontFamily
                font.pixelSize: Style.font.caption
                font.bold: true
                font.letterSpacing: 1.2
                elide: Text.ElideLeft
              }

              Row {
                anchors.right: parent.right
                spacing: Style.space(5)
                Repeater {
                  model: Math.min(12, root.cfg.sessionsBeforeLongBreak || 0)
                  Rectangle {
                    required property int index
                    readonly property bool current: (root.phase === "focus" || root.phase === "idle") && index === root.doneInSet
                    width: Style.space(9)
                    height: width
                    radius: width / 2
                    color: index < root.doneInSet ? uiCtx.fg : "transparent"
                    border.width: Math.max(1, Style.space(1.5))
                    border.color: current || index < root.doneInSet ? uiCtx.fg : Qt.rgba(uiCtx.fg.r, uiCtx.fg.g, uiCtx.fg.b, 0.35)
                  }
                }
              }

              Text {
                width: parent.width
                horizontalAlignment: Text.AlignRight
                textFormat: Text.PlainText
                text: uiCtx.tn("hero.today", root.todayCount).arg(uiCtx.num(root.todayCount))
                color: uiCtx.fg
                opacity: 0.6
                font.family: uiCtx.fontFamily
                font.pixelSize: Style.font.caption
                elide: Text.ElideLeft
              }
            }
          }

          // ---------- Progress ----------
          Item {
            visible: root.online
            width: parent.width
            implicitHeight: Style.space(6)

            Rectangle {
              id: track
              anchors.fill: parent
              radius: height / 2
              color: Qt.rgba(uiCtx.fg.r, uiCtx.fg.g, uiCtx.fg.b, 0.12)
            }
            Rectangle {
              anchors.left: track.left
              anchors.verticalCenter: track.verticalCenter
              height: track.height
              radius: track.radius
              color: uiCtx.fg
              visible: root.progress > 0
              width: Math.max(track.height, track.width * root.progress)
              Behavior on width { NumberAnimation { duration: Style.duration(240); easing.type: Easing.OutCubic } }
            }
          }

          PanelSeparator { visible: root.online; width: parent.width; foreground: uiCtx.fg }

          // ---------- Pages ----------
          Chips {
            visible: root.online
            ctx: uiCtx
            options: [
              { value: "timer", label: uiCtx.t("tab.timer"), icon: Model.GLYPH.timer },
              { value: "noise", label: uiCtx.t("tab.noise"), icon: Model.GLYPH.headphones },
              { value: "alerts", label: uiCtx.t("tab.alerts"), icon: Model.GLYPH.bell }
            ]
            value: root.page
            onPicked: function(v) { root.page = v }
          }

          // Keeps the tabs apart from the first row of each page (on Timer,
          // the start / reset / skip buttons, which look like tabs).
          PanelSeparator { visible: root.online; width: parent.width; foreground: uiCtx.fg }

          TimerPage { ctx: uiCtx; visible: root.online && root.page === "timer" }
          NoisePage { ctx: uiCtx; visible: root.online && root.page === "noise" }
          AlertsPage {
            id: alertsPage
            ctx: uiCtx
            visible: root.online && root.page === "alerts"
            onEditingDone: keys.forceActiveFocus()
          }
        }
      }
    }
  }
}
