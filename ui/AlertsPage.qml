import QtQuick
import qs.Commons
import qs.Ui

Column {
  id: page

  property var ctx: null
  readonly property var svc: ctx ? ctx.svc : null
  readonly property var cfg: ctx ? ctx.cfg : ({})
  // While the path field has focus the panel's key shortcuts stand aside.
  readonly property bool editing: pathField.activeFocus
  signal editingDone()

  width: parent ? parent.width : 0
  spacing: Style.space(10)

  PanelSectionHeader { text: page.ctx.t("alerts.focusEnd"); foreground: page.ctx.fg; fontFamily: page.ctx.fontFamily }
  SoundPicker {
    ctx: page.ctx
    value: page.cfg.focusEndSound || ""
    onPicked: function(v) {
      page.svc.update({ focusEndSound: v })
      page.svc.previewAlert(v)
    }
    onPreview: function(v) { page.svc.previewAlert(v) }
  }

  PanelSectionHeader { text: page.ctx.t("alerts.breakEnd"); foreground: page.ctx.fg; fontFamily: page.ctx.fontFamily }
  SoundPicker {
    ctx: page.ctx
    value: page.cfg.breakEndSound || ""
    onPicked: function(v) {
      page.svc.update({ breakEndSound: v })
      page.svc.previewAlert(v)
    }
    onPreview: function(v) { page.svc.previewAlert(v) }
  }

  SliderRow {
    ctx: page.ctx
    label: page.ctx.t("alerts.volume")
    value: page.cfg.alertVolume || 0
    minimum: 0; maximum: 100; step: 5
    format: function(v) { return page.ctx.percent(v) }
    onCommitted: function(v) {
      page.svc.update({ alertVolume: v })
      page.svc.previewAlert(page.cfg.focusEndSound)
    }
  }

  Column {
    width: parent.width
    spacing: Style.space(4)

    Text {
      textFormat: Text.PlainText
      text: page.ctx.t("alerts.customPath")
      color: page.ctx.fg
      opacity: 0.75
      font.family: page.ctx.fontFamily
      font.pixelSize: Style.font.bodySmall
    }
    TextField {
      id: pathField
      width: parent.width
      foreground: page.ctx.fg
      font.family: page.ctx.fontFamily
      font.pixelSize: Style.font.bodySmall
      placeholderText: page.ctx.t("alerts.customPlaceholder")
      text: page.cfg.customSoundPath || ""
      onEditingFinished: if (text !== (page.cfg.customSoundPath || "")) page.svc.update({ customSoundPath: text })
      Keys.onEscapePressed: page.editingDone()
      Keys.onReturnPressed: page.editingDone()
    }
  }

  SwitchRow {
    ctx: page.ctx
    label: page.ctx.t("alerts.notify")
    checked: !!page.cfg.desktopNotifications
    onToggled: page.svc.update({ desktopNotifications: !page.cfg.desktopNotifications })
  }
  SwitchRow {
    ctx: page.ctx
    label: page.ctx.t("alerts.barTime")
    checked: !!page.cfg.showTimeInBar
    onToggled: page.svc.update({ showTimeInBar: !page.cfg.showTimeInBar })
  }

  Note { ctx: page.ctx; text: page.ctx.t("alerts.note") }
}
