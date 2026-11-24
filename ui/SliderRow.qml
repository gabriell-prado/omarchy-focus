import QtQuick
import qs.Commons
import qs.Ui

// Label and live value above a slider; reports the value once, on release.
Column {
  id: row

  property var ctx: null
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
      text: row.label
      color: row.ctx ? row.ctx.fg : "white"
      opacity: 0.75
      font.family: row.ctx ? row.ctx.fontFamily : Style.font.family
      font.pixelSize: Style.font.bodySmall
      elide: Text.ElideRight
    }
    Text {
      id: valueText
      anchors.right: parent.right
      textFormat: Text.PlainText
      text: row.format(slider.dragging ? slider.liveValue : row.value)
      color: row.ctx ? row.ctx.fg : "white"
      font.family: row.ctx ? row.ctx.fontFamily : Style.font.family
      font.pixelSize: Style.font.bodySmall
      font.bold: true
    }
  }

  PanelSlider {
    id: slider
    width: parent.width
    bar: row.ctx ? row.ctx.bar : null
    value: row.value
    minimum: row.minimum
    maximum: row.maximum
    step: row.step
    integer: true
    tickCount: row.tickCount
    onReleased: function(v) {
      var snapped = Math.round(v / row.step) * row.step
      if (snapped !== row.value) row.committed(snapped)
    }
  }
}
