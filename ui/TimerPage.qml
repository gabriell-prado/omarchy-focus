import QtQuick
import qs.Commons
import qs.Ui
import "../Model.js" as Model

Column {
  id: page

  property var ctx: null
  readonly property var svc: ctx ? ctx.svc : null
  readonly property var cfg: ctx ? ctx.cfg : ({})
  readonly property var presets: ctx && ctx.catalog ? ctx.catalog.timerPresets : []
  readonly property string current: svc ? svc.timerPreset : "custom"
  readonly property var preset: Model.find(presets, current)

  width: parent ? parent.width : 0
  spacing: Style.space(10)

  PanelSectionHeader { text: page.ctx.t("timer.session"); foreground: page.ctx.fg; fontFamily: page.ctx.fontFamily }

  // ---------- Start / pause · reset · skip ----------
  Row {
    id: controls
    width: parent.width
    spacing: Style.space(6)
    readonly property real cell: (width - spacing * 2) / 3
    readonly property string phase: page.svc ? page.svc.phase : "idle"
    readonly property bool running: !!(page.svc && page.svc.running)
    readonly property bool paused: !!(page.svc && page.svc.paused)

    PanelButton {
      ctx: page.ctx
      width: controls.cell
      iconText: controls.running ? Model.GLYPH.pause : Model.GLYPH.play
      text: page.ctx.t(controls.running ? "action.pause" : (controls.paused ? "action.resume" : "action.start"))
      onClicked: page.svc.toggle()
    }
    PanelButton {
      ctx: page.ctx
      width: controls.cell
      iconText: Model.GLYPH.reset
      text: page.ctx.t("action.reset")
      enabled: controls.phase !== "idle"
      opacity: enabled ? 1 : 0.4
      onClicked: page.svc.reset()
    }
    PanelButton {
      ctx: page.ctx
      width: controls.cell
      iconText: Model.GLYPH.skip
      text: page.ctx.t("action.skip")
      enabled: controls.phase !== "idle"
      opacity: enabled ? 1 : 0.4
      onClicked: page.svc.skip()
    }
  }

  PanelSeparator { width: parent.width; foreground: page.ctx.fg }

  PanelSectionHeader { text: page.ctx.t("timer.method"); foreground: page.ctx.fg; fontFamily: page.ctx.fontFamily }

  Chips {
    ctx: page.ctx
    options: page.presets.map(function(p) { return { value: p.id, label: page.ctx.num(p.focus) + "/" + page.ctx.num(p.short) } })
    value: page.current
    onPicked: function(v) { page.svc.applyTimerPreset(v) }
  }

  InfoCard {
    ctx: page.ctx
    title: page.ctx.t("timer.preset." + page.current + ".name")
    body: page.ctx.t("timer.preset." + page.current + ".note")
    refs: page.preset ? page.preset.refs : (page.ctx.catalog ? page.ctx.catalog.customTimerRefs : "")
  }

  SliderRow {
    ctx: page.ctx
    label: page.ctx.t("timer.focus")
    value: page.cfg.focusMinutes || 25
    minimum: 5; maximum: 120
    format: function(v) { return page.ctx.minutes(Math.round(v)) }
    onCommitted: function(v) { page.svc.update({ focusMinutes: v }) }
  }
  SliderRow {
    ctx: page.ctx
    label: page.ctx.t("timer.shortBreak")
    value: page.cfg.shortBreakMinutes || 5
    minimum: 1; maximum: 30
    format: function(v) { return page.ctx.minutes(Math.round(v)) }
    onCommitted: function(v) { page.svc.update({ shortBreakMinutes: v }) }
  }
  SliderRow {
    ctx: page.ctx
    label: page.ctx.t("timer.longBreak")
    value: page.cfg.longBreakMinutes || 15
    minimum: 5; maximum: 60
    format: function(v) { return page.ctx.minutes(Math.round(v)) }
    onCommitted: function(v) { page.svc.update({ longBreakMinutes: v }) }
  }
  SliderRow {
    ctx: page.ctx
    label: page.ctx.t("timer.sessions")
    value: page.cfg.sessionsBeforeLongBreak || 4
    minimum: 1; maximum: 8
    tickCount: 8
    format: function(v) { var n = Math.round(v); return page.ctx.tn("unit.sessions", n).arg(page.ctx.num(n)) }
    onCommitted: function(v) { page.svc.update({ sessionsBeforeLongBreak: v }) }
  }

  SwitchRow {
    ctx: page.ctx
    label: page.ctx.t("timer.autoBreaks")
    checked: !!page.cfg.autoStartBreaks
    onToggled: page.svc.update({ autoStartBreaks: !page.cfg.autoStartBreaks })
  }
  SwitchRow {
    ctx: page.ctx
    label: page.ctx.t("timer.autoFocus")
    checked: !!page.cfg.autoStartFocus
    onToggled: page.svc.update({ autoStartFocus: !page.cfg.autoStartFocus })
  }

  Note { ctx: page.ctx; text: page.ctx.t("timer.breaks.note") }
}
