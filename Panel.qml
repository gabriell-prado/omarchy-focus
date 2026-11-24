import QtQuick
import Quickshell
import qs.Commons
import qs.Commons as Commons
import qs.Ui
import "Model.js" as Model

// Bar widget and panel. One of these exists per monitor; all of them draw the
// single Service instance and send every action to it.
//
// Bar:   left click opens the panel, right click starts/pauses, middle click
//        turns the focus sound on or off.
// Panel: Space/Enter start/pause, r reset, s skip, n sound, 1-4 or ←/→ pages.
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

  onSvcChanged: if (svc) svc.noteSettings(settings)
  onSettingsChanged: if (svc) svc.noteSettings(settings)

  function update(patch) { if (svc) svc.update(patch) }

  // Choosing or adjusting a sound means wanting to hear it: switch the sound
  // on and, when it follows the timer, audition it until the panel closes.
  function chooseSound(patch) {
    if (!svc) return
    svc.update(Object.assign({ soundEnabled: true }, patch || {}))
    svc.audition()
  }

  function setSoundEnabled(on) {
    if (!svc) return
    if (on) chooseSound({})
    else {
      svc.update({ soundEnabled: false })
      svc.endAudition()
    }
  }

  onOpenedChanged: if (!opened && svc) svc.endAudition()

  readonly property var cfg: svc ? svc.cfg : Model.normalizeSettings(settings)
  readonly property string phase: svc ? svc.phase : "idle"
  readonly property bool running: svc ? svc.running : false
  readonly property bool paused: svc ? svc.paused : false
  readonly property bool fresh: svc ? svc.fresh : true
  readonly property real leftMs: svc ? svc.leftMs : Model.phaseMs("focus", cfg)
  readonly property real progress: svc ? svc.progress : 0
  readonly property int doneInSet: svc ? svc.doneInSet : 0
  readonly property int todayCount: svc ? svc.todayCount : 0
  readonly property string clock: localizeDigits(Model.formatClock(leftMs))

  readonly property string timerPreset: Model.matchTimerPreset(cfg)
  readonly property string soundPreset: Model.matchSoundPreset(cfg)

  // ---------------------------------------------------------------- text helpers

  function localizeDigits(text) {
    return String(text).replace(/[0-9]/g, function(d) { return Commons.I18n.formatNumber(Number(d)) })
  }
  function num(n) { return Commons.I18n.formatNumber(n) }
  function minutesText(n) { return strings.tn("unit.minutes", n).arg(num(n)) }
  function percentText(n) { return strings.t("unit.percent").arg(num(Math.round(n))) }
  function hzText(n) { return strings.t("unit.hz").arg(num(Math.round(n))) }

  readonly property string phaseName: strings.t("phase." + phase)
  readonly property string phaseLine: {
    if (phase === "idle") return strings.t("phase.idle")
    if (paused) return strings.t("hero.paused").arg(phaseName)
    if (!running) return strings.t("hero.upNext").arg(phaseName)
    return phaseName
  }
  readonly property string tooltip: phase === "idle"
    ? strings.t("bar.tooltip.idle")
    : strings.t(running ? "bar.tooltip.running" : "bar.tooltip.paused").arg(phaseName).arg(clock)

  readonly property string soundStatus: {
    if (!svc) return ""
    if (svc.soundFailed) return strings.t("sound.status.failed")
    if (svc.soundPlaying && !svc.soundWanted) return strings.t("sound.status.preview")
    if (svc.soundPlaying) return strings.t("sound.status.playing")
    if (!cfg.soundEnabled) return strings.t("sound.status.off")
    return cfg.soundOnlyDuringFocus ? strings.t("sound.status.waiting") : strings.t("sound.status.starting")
  }

  // ---------------------------------------------------------------- look

  readonly property color fg: bar ? bar.foreground : Commons.Color.foreground
  readonly property color barFg: bar ? bar.barForeground : Commons.Color.foreground
  readonly property string fontFamily: bar ? bar.fontFamily : Style.font.family

  // ---------------------------------------------------------------- pages

  property string page: "timer"
  readonly property var pages: ["timer", "sound", "tune", "alerts"]
  function cyclePage(delta) {
    var i = pages.indexOf(page)
    page = pages[(i + delta + pages.length) % pages.length]
  }

  // ---------------------------------------------------------------- bar button

  readonly property bool showTime: cfg.showTimeInBar && phase !== "idle" && !(bar && bar.vertical)

  implicitWidth: button.implicitWidth
  implicitHeight: button.implicitHeight

  TextMetrics {
    id: clockMetrics
    font.family: root.fontFamily
    font.pixelSize: Style.bar.iconFont
    text: root.clock
  }

  BarIconButton {
    id: button
    anchors.fill: parent
    bar: root.bar
    text: Model.barGlyph(root.phase, root.running, root.fresh)
    labelText: root.showTime ? root.clock : ""
    // The status slot, like Elsewhen and Weather beside the clock; the time,
    // when shown, adds its own width plus BarIconButton's label-to-icon gap.
    slotSize: Style.bar.statusSlot + (root.showTime ? Math.ceil(clockMetrics.advanceWidth) + Style.space(4) : 0)
    foreground: root.running || root.phase === "idle" ? root.barFg : Qt.darker(root.barFg, 1.55)
    tooltipText: root.opened ? "" : root.tooltip
    onPressed: function(b) {
      if (b === Qt.RightButton) {
        if (root.svc) root.svc.toggle()
      } else if (b === Qt.MiddleButton) {
        root.update({ soundEnabled: !root.cfg.soundEnabled })
      } else {
        root.toggle()
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
    contentWidth: panel.fittedContentWidth(Style.space(400))
    contentHeight: panel.fittedContentHeight(panel.loadedContentHeight)

    content: PanelKeyCatcher {
      id: keys
      anchors.fill: parent
      implicitHeight: column.implicitHeight
      blocked: pathField.activeFocus

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
        else if (text === "n") root.setSoundEnabled(!root.cfg.soundEnabled)
        else if (text >= "1" && text <= "4") root.page = root.pages[Number(text) - 1]
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

          // ---------- Hero: clock · phase · session dots ----------
          Item {
            width: parent.width
            implicitHeight: Math.max(heroClock.implicitHeight, heroSide.implicitHeight)

            Text {
              id: heroClock
              anchors.left: parent.left
              anchors.verticalCenter: parent.verticalCenter
              textFormat: Text.PlainText
              text: root.clock
              color: root.fg
              opacity: root.running || root.phase === "idle" ? 1 : 0.6
              font.family: root.fontFamily
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
                color: root.fg
                font.family: root.fontFamily
                font.pixelSize: Style.font.caption
                font.bold: true
                font.letterSpacing: 1.2
                elide: Text.ElideLeft
              }

              Row {
                anchors.right: parent.right
                spacing: Style.space(5)
                Repeater {
                  model: Math.min(12, root.cfg.sessionsBeforeLongBreak)
                  Rectangle {
                    required property int index
                    readonly property bool current: (root.phase === "focus" || root.phase === "idle") && index === root.doneInSet
                    width: Style.space(9)
                    height: width
                    radius: width / 2
                    color: index < root.doneInSet ? root.fg : "transparent"
                    border.width: Math.max(1, Style.space(1.5))
                    border.color: current || index < root.doneInSet ? root.fg : Qt.rgba(root.fg.r, root.fg.g, root.fg.b, 0.35)
                  }
                }
              }

              Text {
                width: parent.width
                horizontalAlignment: Text.AlignRight
                textFormat: Text.PlainText
                text: strings.tn("hero.today", root.todayCount).arg(root.num(root.todayCount))
                color: root.fg
                opacity: 0.6
                font.family: root.fontFamily
                font.pixelSize: Style.font.caption
                elide: Text.ElideLeft
              }
            }
          }

          // ---------- Progress ----------
          Item {
            width: parent.width
            implicitHeight: Style.space(6)

            Rectangle {
              id: track
              anchors.fill: parent
              radius: height / 2
              color: Qt.rgba(root.fg.r, root.fg.g, root.fg.b, 0.12)
            }
            Rectangle {
              anchors.left: track.left
              anchors.verticalCenter: track.verticalCenter
              height: track.height
              radius: track.radius
              color: root.fg
              visible: root.progress > 0
              width: Math.max(track.height, track.width * root.progress)
              Behavior on width { NumberAnimation { duration: Style.duration(240); easing.type: Easing.OutCubic } }
            }
          }

          // ---------- Controls ----------
          Row {
            id: controls
            width: parent.width
            spacing: Style.space(6)
            readonly property real cell: (width - spacing * 2) / 3

            PanelButton {
              width: controls.cell
              iconText: root.running ? Model.GLYPH.pause : Model.GLYPH.play
              text: strings.t(root.running ? "action.pause" : (root.paused ? "action.resume" : "action.start"))
              onClicked: if (root.svc) root.svc.toggle()
            }
            PanelButton {
              width: controls.cell
              iconText: Model.GLYPH.reset
              text: strings.t("action.reset")
              enabled: root.phase !== "idle"
              opacity: enabled ? 1 : 0.4
              onClicked: if (root.svc) root.svc.reset()
            }
            PanelButton {
              width: controls.cell
              iconText: Model.GLYPH.skip
              text: strings.t("action.skip")
              enabled: root.phase !== "idle"
              opacity: enabled ? 1 : 0.4
              onClicked: if (root.svc) root.svc.skip()
            }
          }

          PanelSeparator { width: parent.width; foreground: root.fg }

          // ---------- Pages ----------
          Chips {
            options: [
              { value: "timer", label: strings.t("tab.timer"), icon: Model.GLYPH.timer },
              { value: "sound", label: strings.t("tab.sound"), icon: Model.GLYPH.headphones },
              { value: "tune", label: strings.t("tab.tune"), icon: Model.GLYPH.tune },
              { value: "alerts", label: strings.t("tab.alerts"), icon: Model.GLYPH.bell }
            ]
            value: root.page
            onPicked: function(v) { root.page = v }
          }

          // ================= Timer =================
          Column {
            visible: root.page === "timer"
            width: parent.width
            spacing: Style.space(10)

            PanelSectionHeader { text: strings.t("timer.method"); foreground: root.fg; fontFamily: root.fontFamily }

            Chips {
              options: Model.TIMER_PRESETS.map(function(p) {
                return { value: p.id, label: root.num(p.focus) + "/" + root.num(p.short) }
              })
              value: root.timerPreset
              onPicked: function(v) { root.update(Model.timerPresetPatch(v)) }
            }

            InfoCard {
              title: strings.t("timer.preset." + root.timerPreset + ".name")
              body: strings.t("timer.preset." + root.timerPreset + ".note")
              refs: Model.timerRefs(root.timerPreset)
            }

            SliderRow {
              label: strings.t("timer.focus")
              value: root.cfg.focusMinutes
              minimum: 5; maximum: 120
              format: function(v) { return root.minutesText(Math.round(v)) }
              onCommitted: function(v) { root.update({ focusMinutes: v }) }
            }
            SliderRow {
              label: strings.t("timer.shortBreak")
              value: root.cfg.shortBreakMinutes
              minimum: 1; maximum: 30
              format: function(v) { return root.minutesText(Math.round(v)) }
              onCommitted: function(v) { root.update({ shortBreakMinutes: v }) }
            }
            SliderRow {
              label: strings.t("timer.longBreak")
              value: root.cfg.longBreakMinutes
              minimum: 5; maximum: 60
              format: function(v) { return root.minutesText(Math.round(v)) }
              onCommitted: function(v) { root.update({ longBreakMinutes: v }) }
            }
            SliderRow {
              label: strings.t("timer.sessions")
              value: root.cfg.sessionsBeforeLongBreak
              minimum: 1; maximum: 8
              tickCount: 8
              format: function(v) { var n = Math.round(v); return strings.tn("unit.sessions", n).arg(root.num(n)) }
              onCommitted: function(v) { root.update({ sessionsBeforeLongBreak: v }) }
            }

            SwitchRow {
              label: strings.t("timer.autoBreaks")
              checked: root.cfg.autoStartBreaks
              onToggled: root.update({ autoStartBreaks: !root.cfg.autoStartBreaks })
            }
            SwitchRow {
              label: strings.t("timer.autoFocus")
              checked: root.cfg.autoStartFocus
              onToggled: root.update({ autoStartFocus: !root.cfg.autoStartFocus })
            }

            Note { text: strings.t("timer.breaks.note") }
          }

          // ================= Sound =================
          Column {
            visible: root.page === "sound"
            width: parent.width
            spacing: Style.space(10)

            Item {
              width: parent.width
              implicitHeight: Math.max(soundTitle.implicitHeight, master.implicitHeight)

              Column {
                id: soundTitle
                anchors.left: parent.left
                anchors.right: master.left
                anchors.rightMargin: Style.space(10)
                anchors.verticalCenter: parent.verticalCenter
                spacing: Style.space(2)

                Text {
                  width: parent.width
                  textFormat: Text.PlainText
                  text: strings.t("sound.title")
                  color: root.fg
                  font.family: root.fontFamily
                  font.pixelSize: Style.font.title
                  font.bold: true
                  elide: Text.ElideRight
                }
                Text {
                  width: parent.width
                  textFormat: Text.PlainText
                  text: root.soundStatus
                  color: root.fg
                  opacity: 0.6
                  font.family: root.fontFamily
                  font.pixelSize: Style.font.caption
                  elide: Text.ElideRight
                }
              }

              ToggleSwitch {
                id: master
                anchors.right: parent.right
                anchors.verticalCenter: parent.verticalCenter
                foreground: root.fg
                checked: root.cfg.soundEnabled
                onToggled: root.setSoundEnabled(!root.cfg.soundEnabled)
              }
            }

            PanelSectionHeader { text: strings.t("sound.presets"); foreground: root.fg; fontFamily: root.fontFamily }

            Column {
              width: parent.width
              spacing: Style.space(4)

              Repeater {
                model: Model.SOUND_PRESETS

                PanelButton {
                  id: presetRow
                  required property var modelData
                  width: parent.width
                  leftAlign: true
                  iconText: modelData.beatMode === "off" ? Model.GLYPH.wave : Model.GLYPH.headphones
                  text: strings.t("preset." + modelData.id + ".name")
                  active: root.soundPreset === modelData.id
                  onClicked: root.chooseSound(Model.soundPresetPatch(modelData.id))

                  Text {
                    anchors.right: parent.right
                    anchors.rightMargin: Style.spacing.controlPaddingX
                    anchors.verticalCenter: parent.verticalCenter
                    textFormat: Text.PlainText
                    text: strings.t("evidence." + presetRow.modelData.evidence + ".short")
                    color: root.fg
                    opacity: 0.55
                    font.family: root.fontFamily
                    font.pixelSize: Style.font.caption
                  }
                }
              }
            }

            InfoCard {
              readonly property var preset: Model.soundPreset(root.soundPreset)
              title: preset ? strings.t("preset." + preset.id + ".name") : strings.t("sound.custom")
              badge: preset ? strings.t("evidence." + preset.evidence) : ""
              body: preset ? strings.t("preset." + preset.id + ".note") : strings.t("sound.custom.note")
              refs: preset ? preset.refs : ""
            }

            Note {
              visible: root.cfg.beatMode === "binaural"
              text: Model.GLYPH.headphones + "  " + strings.t("tune.headphones")
              opacity: 0.85
            }

            SliderRow {
              visible: root.cfg.noiseColor !== "off"
              label: strings.t("sound.noiseVolume")
              value: root.cfg.noiseVolume
              minimum: 0; maximum: 100; step: 5
              format: function(v) { return root.percentText(v) }
              onCommitted: function(v) { root.chooseSound({ noiseVolume: v }) }
            }
            SliderRow {
              visible: root.cfg.beatMode !== "off"
              label: strings.t("sound.toneVolume")
              value: root.cfg.toneVolume
              minimum: 0; maximum: 100; step: 5
              format: function(v) { return root.percentText(v) }
              onCommitted: function(v) { root.chooseSound({ toneVolume: v }) }
            }

            SwitchRow {
              label: strings.t("sound.onlyFocus")
              checked: root.cfg.soundOnlyDuringFocus
              onToggled: root.update({ soundOnlyDuringFocus: !root.cfg.soundOnlyDuringFocus })
            }

            Note { text: strings.t("sound.volume.note") }
          }

          // ================= Tune =================
          Column {
            visible: root.page === "tune"
            width: parent.width
            spacing: Style.space(10)

            PanelSectionHeader { text: strings.t("tune.noise"); foreground: root.fg; fontFamily: root.fontFamily }

            Chips {
              options: Model.NOISE_COLORS.map(function(c) { return { value: c, label: strings.t("noise." + c) } })
              value: root.cfg.noiseColor
              onPicked: function(v) { root.chooseSound({ noiseColor: v }) }
            }

            Note { text: strings.t("tune.noise.note") }

            PanelSeparator { width: parent.width; foreground: root.fg }

            PanelSectionHeader { text: strings.t("tune.tone"); foreground: root.fg; fontFamily: root.fontFamily }

            Chips {
              options: Model.BEAT_MODES.map(function(m) { return { value: m, label: strings.t("beat." + m) } })
              value: root.cfg.beatMode
              onPicked: function(v) { root.chooseSound({ beatMode: v }) }
            }

            SliderRow {
              visible: root.cfg.beatMode !== "off"
              label: strings.t("tune.beat")
              value: root.cfg.beatHz
              minimum: 1; maximum: 45
              format: function(v) { return root.hzText(v) + " · " + strings.t("band." + Model.bandFor(Math.round(v))) }
              onCommitted: function(v) { root.chooseSound({ beatHz: v }) }
            }
            SliderRow {
              visible: root.cfg.beatMode !== "off"
              label: strings.t("tune.carrier")
              value: root.cfg.carrierHz
              minimum: 100; maximum: 600; step: 10
              format: function(v) { return root.hzText(v) }
              onCommitted: function(v) { root.chooseSound({ carrierHz: Math.round(v / 10) * 10 }) }
            }

            Note {
              visible: root.cfg.beatMode === "binaural"
              text: Model.GLYPH.headphones + "  " + strings.t("tune.headphones")
              opacity: 0.85
            }
            Note {
              visible: root.cfg.beatMode !== "off"
              text: strings.t("tune.carrier.note")
            }
            Note { text: strings.t("tune.beat.note") }
          }

          // ================= Alerts =================
          Column {
            visible: root.page === "alerts"
            width: parent.width
            spacing: Style.space(10)

            PanelSectionHeader { text: strings.t("alerts.focusEnd"); foreground: root.fg; fontFamily: root.fontFamily }
            SoundPicker {
              value: root.cfg.focusEndSound
              onPicked: function(v) {
                root.update({ focusEndSound: v })
                if (root.svc) root.svc.previewAlert(v)
              }
            }

            PanelSectionHeader { text: strings.t("alerts.breakEnd"); foreground: root.fg; fontFamily: root.fontFamily }
            SoundPicker {
              value: root.cfg.breakEndSound
              onPicked: function(v) {
                root.update({ breakEndSound: v })
                if (root.svc) root.svc.previewAlert(v)
              }
            }

            SliderRow {
              label: strings.t("alerts.volume")
              value: root.cfg.alertVolume
              minimum: 0; maximum: 100; step: 5
              format: function(v) { return root.percentText(v) }
              onCommitted: function(v) {
                root.update({ alertVolume: v })
                if (root.svc) root.svc.previewAlert(root.cfg.focusEndSound)
              }
            }

            Column {
              width: parent.width
              spacing: Style.space(4)

              Text {
                textFormat: Text.PlainText
                text: strings.t("alerts.customPath")
                color: root.fg
                opacity: 0.75
                font.family: root.fontFamily
                font.pixelSize: Style.font.bodySmall
              }
              TextField {
                id: pathField
                width: parent.width
                foreground: root.fg
                font.family: root.fontFamily
                font.pixelSize: Style.font.bodySmall
                placeholderText: strings.t("alerts.customPlaceholder")
                text: root.cfg.customSoundPath
                onEditingFinished: if (text !== root.cfg.customSoundPath) root.update({ customSoundPath: text })
                Keys.onEscapePressed: keys.forceActiveFocus()
                Keys.onReturnPressed: keys.forceActiveFocus()
              }
            }

            SwitchRow {
              label: strings.t("alerts.notify")
              checked: root.cfg.desktopNotifications
              onToggled: root.update({ desktopNotifications: !root.cfg.desktopNotifications })
            }
            SwitchRow {
              label: strings.t("alerts.barTime")
              checked: root.cfg.showTimeInBar
              onToggled: root.update({ showTimeInBar: !root.cfg.showTimeInBar })
            }

            Note { text: strings.t("alerts.note") }
          }
        }
      }
    }
  }

  // ---------------------------------------------------------------- components

  component PanelButton: Button {
    foreground: root.fg
    fontFamily: root.fontFamily
    fontSize: Style.font.bodySmall
    iconSize: Style.font.title
    bordered: true
    horizontalPadding: Style.spacing.controlPaddingX
    verticalPadding: Style.spacing.controlPaddingY + Style.space(2)
  }

  // Equal-width, pick-one row of buttons (the power panel's profile picker).
  component Chips: Row {
    id: chips
    property var options: []
    property string value: ""
    signal picked(string value)

    width: parent ? parent.width : 0
    spacing: Style.space(6)
    readonly property real cell: options.length > 0 ? (width - spacing * (options.length - 1)) / options.length : 0

    Repeater {
      model: chips.options
      PanelButton {
        required property var modelData
        width: chips.cell
        text: modelData.label
        iconText: modelData.icon || ""
        active: chips.value === modelData.value
        onClicked: chips.picked(modelData.value)
      }
    }
  }

  component SliderRow: Column {
    id: sliderRow
    property string label: ""
    property real value: 0
    property real minimum: 0
    property real maximum: 100
    property real step: 1
    property int tickCount: 0
    property var format: function(v) { return String(Math.round(v)) }
    signal committed(int value)

    width: parent ? parent.width : 0
    spacing: Style.space(2)

    Item {
      width: parent.width
      implicitHeight: Math.max(nameText.implicitHeight, valueText.implicitHeight)

      Text {
        id: nameText
        anchors.left: parent.left
        anchors.right: valueText.left
        anchors.rightMargin: Style.space(8)
        textFormat: Text.PlainText
        text: sliderRow.label
        color: root.fg
        opacity: 0.75
        font.family: root.fontFamily
        font.pixelSize: Style.font.bodySmall
        elide: Text.ElideRight
      }
      Text {
        id: valueText
        anchors.right: parent.right
        textFormat: Text.PlainText
        text: sliderRow.format(slider.dragging ? slider.liveValue : sliderRow.value)
        color: root.fg
        font.family: root.fontFamily
        font.pixelSize: Style.font.bodySmall
        font.bold: true
      }
    }

    PanelSlider {
      id: slider
      width: parent.width
      bar: root.bar
      value: sliderRow.value
      minimum: sliderRow.minimum
      maximum: sliderRow.maximum
      step: sliderRow.step
      integer: true
      tickCount: sliderRow.tickCount
      onReleased: function(v) {
        var snapped = Math.round(v / sliderRow.step) * sliderRow.step
        if (snapped !== sliderRow.value) sliderRow.committed(snapped)
      }
    }
  }

  component SwitchRow: Item {
    id: switchRow
    property string label: ""
    property bool checked: false
    signal toggled()

    width: parent ? parent.width : 0
    implicitHeight: Math.max(switchText.implicitHeight, switchControl.implicitHeight)

    Text {
      id: switchText
      anchors.left: parent.left
      anchors.right: switchControl.left
      anchors.rightMargin: Style.space(10)
      anchors.verticalCenter: parent.verticalCenter
      textFormat: Text.PlainText
      text: switchRow.label
      color: root.fg
      font.family: root.fontFamily
      font.pixelSize: Style.font.bodySmall
      wrapMode: Text.WordWrap

      MouseArea {
        anchors.fill: parent
        cursorShape: Qt.PointingHandCursor
        onClicked: switchRow.toggled()
      }
    }

    ToggleSwitch {
      id: switchControl
      anchors.right: parent.right
      anchors.verticalCenter: parent.verticalCenter
      foreground: root.fg
      checked: switchRow.checked
      onToggled: switchRow.toggled()
    }
  }

  component Note: Text {
    width: parent ? parent.width : 0
    textFormat: Text.PlainText
    wrapMode: Text.WordWrap
    color: root.fg
    opacity: 0.65
    font.family: root.fontFamily
    font.pixelSize: Style.font.caption
  }

  // Title, evidence badge, explanation and citations for the chosen preset.
  component InfoCard: Rectangle {
    id: card
    property string title: ""
    property string badge: ""
    property string body: ""
    property string refs: ""

    width: parent ? parent.width : 0
    implicitHeight: cardColumn.implicitHeight + Style.space(20)
    radius: Style.cornerRadius > 0 ? Style.space(6) : 0
    color: Qt.rgba(root.fg.r, root.fg.g, root.fg.b, 0.06)

    Column {
      id: cardColumn
      anchors.left: parent.left
      anchors.right: parent.right
      anchors.top: parent.top
      anchors.margins: Style.space(10)
      spacing: Style.space(5)

      Item {
        width: parent.width
        implicitHeight: Math.max(cardTitle.implicitHeight, cardBadge.implicitHeight)

        Text {
          id: cardTitle
          anchors.left: parent.left
          anchors.right: cardBadge.left
          anchors.rightMargin: Style.space(8)
          anchors.verticalCenter: parent.verticalCenter
          textFormat: Text.PlainText
          text: card.title
          color: root.fg
          font.family: root.fontFamily
          font.pixelSize: Style.font.body
          font.bold: true
          elide: Text.ElideRight
        }
        Text {
          id: cardBadge
          anchors.right: parent.right
          anchors.verticalCenter: parent.verticalCenter
          visible: card.badge !== ""
          textFormat: Text.PlainText
          text: card.badge.toUpperCase()
          color: root.fg
          opacity: 0.7
          font.family: root.fontFamily
          font.pixelSize: Style.font.caption
          font.bold: true
          font.letterSpacing: 0.8
        }
      }

      Text {
        width: parent.width
        textFormat: Text.PlainText
        text: card.body
        color: root.fg
        opacity: 0.85
        wrapMode: Text.WordWrap
        font.family: root.fontFamily
        font.pixelSize: Style.font.bodySmall
      }

      Text {
        width: parent.width
        visible: card.refs !== ""
        textFormat: Text.PlainText
        text: card.refs
        color: root.fg
        opacity: 0.5
        wrapMode: Text.WordWrap
        font.family: root.fontFamily
        font.pixelSize: Style.font.caption
        font.italic: true
      }
    }
  }

  // ‹ sound name ›  The arrows pick the previous/next sound and play it; a
  // click on the name plays it again. The service plays one alert at a time,
  // so a new one always cuts off the one still sounding.
  component SoundPicker: Row {
    id: picker
    property string value: ""
    signal picked(string value)

    width: parent ? parent.width : 0
    spacing: Style.space(6)
    readonly property int index: Math.max(0, Model.ALERT_IDS.indexOf(value))
    readonly property real small: Style.spacing.controlHeight + Style.space(10)
    // The arrows' glyphs are taller than the name's text, so all three take
    // the tallest natural height and line up as one row.
    readonly property real rowHeight: Math.max(previous.implicitHeight, name.implicitHeight, next.implicitHeight)

    function step(delta) {
      var ids = Model.ALERT_IDS
      picked(ids[(index + delta + ids.length) % ids.length])
    }

    PanelButton {
      id: previous
      width: picker.small
      height: picker.rowHeight
      iconText: Commons.I18n.rightToLeft ? Model.GLYPH.right : Model.GLYPH.left
      onClicked: picker.step(-1)
    }
    PanelButton {
      id: name
      width: picker.width - picker.small * 2 - picker.spacing * 2
      height: picker.rowHeight
      text: picker.value === "custom" && root.cfg.customSoundPath === ""
        ? strings.t("alert.custom.unset")
        : strings.t("alert." + picker.value)
      tooltipText: strings.t("alerts.preview")
      onClicked: if (root.svc) root.svc.previewAlert(picker.value)
    }
    PanelButton {
      id: next
      width: picker.small
      height: picker.rowHeight
      iconText: Commons.I18n.rightToLeft ? Model.GLYPH.left : Model.GLYPH.right
      onClicked: picker.step(1)
    }
  }
}
