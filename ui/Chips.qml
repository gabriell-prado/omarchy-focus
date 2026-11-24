import QtQuick
import qs.Commons

// Equal-width, pick-one row of buttons (the power panel's profile picker).
// options: [{ value, label, icon? }]
Row {
  id: chips

  property var ctx: null
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
      ctx: chips.ctx
      width: chips.cell
      text: modelData.label
      iconText: modelData.icon || ""
      active: chips.value === modelData.value
      onClicked: chips.picked(modelData.value)
    }
  }
}
