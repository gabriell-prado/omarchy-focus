import QtQuick
import qs.Commons

// Small explanatory text.
Text {
  property var ctx: null

  width: parent ? parent.width : 0
  textFormat: Text.PlainText
  wrapMode: Text.WordWrap
  color: ctx ? ctx.fg : "white"
  opacity: 0.65
  font.family: ctx ? ctx.fontFamily : Style.font.family
  font.pixelSize: Style.font.caption
}
