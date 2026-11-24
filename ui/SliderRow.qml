import QtQuick
import qs.Commons
import qs.Ui

// Label and live value above a slider. Reports the value once, on release;
// with `live`, also every time the knob reaches a new step while dragging, so
// what it controls (a volume, a tone) follows the hand.
Column {
  id: row

  property var ctx: null
  property string label: ""
  property real value: 0
  property real minimum: 0
  property real maximum: 100
  property real step: 1
  property int tickCount: 0
  property bool live: false
  property var format: function(v) { return String(Math.round(v)) }
  signal committed(int value)

  // The last value sent during this drag, so each step is sent once.
  property real _sent: NaN

  function snap(v) {
    return Math.max(minimum, Math.min(maximum, Math.round(v / step) * step))
  }

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
    onMoved: function(v) {
      if (!row.live) return
      var snapped = row.snap(v)
      var last = isNaN(row._sent) ? row.value : row._sent
      if (snapped === last) return
      row._sent = snapped
      row.committed(snapped)
    }
    onReleased: function(v) {
      var snapped = row.snap(v)
      var last = isNaN(row._sent) ? row.value : row._sent
      row._sent = NaN
      if (snapped !== last) row.committed(snapped)
    }
  }
}
