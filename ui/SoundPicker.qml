import QtQuick
import qs.Commons
import qs.Commons as Commons
import "../Model.js" as Model

// ‹ sound name ›  The arrows pick the previous/next sound and play it; a
// click on the name plays it again. The engine plays one alert at a time, so
// a new one always cuts off the one still sounding.
Row {
  id: picker

  property var ctx: null
  property string value: ""
  signal picked(string value)
  signal preview(string value)

  readonly property var ids: ctx && ctx.catalog ? ctx.catalog.alerts : []
  readonly property int index: Math.max(0, ids.indexOf(value))
  readonly property real small: Style.spacing.controlHeight + Style.space(10)
  // The arrows' glyphs are taller than the name's text, so all three take
  // the tallest natural height and line up as one row.
  readonly property real rowHeight: Math.max(previous.implicitHeight, name.implicitHeight, next.implicitHeight)

  width: parent ? parent.width : 0
  spacing: Style.space(6)

  function step(delta) {
    if (ids.length === 0) return
    picked(ids[(index + delta + ids.length) % ids.length])
  }

  PanelButton {
    id: previous
    ctx: picker.ctx
    width: picker.small
    height: picker.rowHeight
    iconText: Commons.I18n.rightToLeft ? Model.GLYPH.right : Model.GLYPH.left
    onClicked: picker.step(-1)
  }
  PanelButton {
    id: name
    ctx: picker.ctx
    width: picker.width - picker.small * 2 - picker.spacing * 2
    height: picker.rowHeight
    text: !picker.ctx ? ""
      : picker.value === "custom" && !picker.ctx.cfg.customSoundPath
        ? picker.ctx.t("alert.custom.unset")
        : picker.ctx.t("alert." + picker.value)
    tooltipText: picker.ctx ? picker.ctx.t("alerts.preview") : ""
    onClicked: picker.preview(picker.value)
  }
  PanelButton {
    id: next
    ctx: picker.ctx
    width: picker.small
    height: picker.rowHeight
    iconText: Commons.I18n.rightToLeft ? Model.GLYPH.left : Model.GLYPH.right
    onClicked: picker.step(1)
  }
}
