import QtQuick
import qs.Commons
import qs.Commons as Commons
import qs.Ui

// A label with a switch at the end; clicking either flips it.
Item {
  id: row

  property var ctx: null
  property string label: ""
  property bool checked: false
  signal toggled()

  width: parent ? parent.width : 0
  implicitHeight: Math.max(labelText.implicitHeight, control.implicitHeight)

  Text {
    id: labelText
    anchors.left: parent.left
    anchors.right: control.left
    anchors.rightMargin: Style.space(10)
    anchors.verticalCenter: parent.verticalCenter
    textFormat: Text.PlainText
    text: row.label
    color: row.ctx ? row.ctx.fg : "white"
    font.family: row.ctx ? row.ctx.fontFamily : Style.font.family
    font.pixelSize: Style.font.bodySmall
    wrapMode: Text.WordWrap

    MouseArea {
      anchors.fill: parent
      cursorShape: Qt.PointingHandCursor
      onClicked: row.toggled()
    }
  }

  ToggleSwitch {
    id: control
    anchors.right: parent.right
    anchors.verticalCenter: parent.verticalCenter
    foreground: row.ctx ? row.ctx.fg : Commons.Color.foreground
    checked: row.checked
    onToggled: row.toggled()
  }
}
