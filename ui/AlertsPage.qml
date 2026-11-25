import QtQuick
import qs.Commons
import qs.Commons as Commons
import qs.Ui
import "../Model.js" as Model

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

  // Notifications are on here, but Omarchy's Do Not Disturb keeps them off
  // the screen: say so, and offer to switch it off.
  Rectangle {
    id: dndWarning
    readonly property color warn: Commons.Color.urgent
    visible: !!(page.svc && page.svc.doNotDisturb && page.cfg.desktopNotifications)
    width: parent.width
    implicitHeight: warningColumn.implicitHeight + Style.space(20)
    radius: Style.cornerRadius > 0 ? Style.space(6) : 0
    color: Qt.rgba(warn.r, warn.g, warn.b, 0.12)
    border.width: Math.max(1, Style.space(1))
    border.color: Qt.rgba(warn.r, warn.g, warn.b, 0.6)

    Column {
      id: warningColumn
      anchors.left: parent.left
      anchors.right: parent.right
      anchors.top: parent.top
      anchors.margins: Style.space(10)
      spacing: Style.space(8)

      Row {
        width: parent.width
        spacing: Style.space(8)

        Text {
          id: warningIcon
          textFormat: Text.PlainText
          text: Model.GLYPH.bellOff
          color: dndWarning.warn
          font.family: page.ctx.fontFamily
          font.pixelSize: Style.font.title
        }
        Text {
          width: parent.width - warningIcon.width - parent.spacing
          textFormat: Text.PlainText
          text: page.ctx.t("alerts.dnd.warning")
          color: page.ctx.fg
          wrapMode: Text.WordWrap
          font.family: page.ctx.fontFamily
          font.pixelSize: Style.font.bodySmall
        }
      }

      PanelButton {
        ctx: page.ctx
        width: parent.width
        text: page.ctx.t("alerts.dnd.off")
        onClicked: page.svc.turnOffDoNotDisturb()
      }
    }
  }

  Note { ctx: page.ctx; text: page.ctx.t("alerts.note") }
}
