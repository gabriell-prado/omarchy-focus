import QtQuick
import qs.Commons

// Title, evidence badge, explanation and citations for the chosen preset.
Rectangle {
  id: card

  property var ctx: null
  property string title: ""
  property string badge: ""
  property string body: ""
  property string refs: ""
  readonly property color fg: ctx ? ctx.fg : "white"
  readonly property string fontFamily: ctx ? ctx.fontFamily : Style.font.family

  width: parent ? parent.width : 0
  implicitHeight: content.implicitHeight + Style.space(20)
  radius: Style.cornerRadius > 0 ? Style.space(6) : 0
  color: Qt.rgba(fg.r, fg.g, fg.b, 0.06)

  Column {
    id: content
    anchors.left: parent.left
    anchors.right: parent.right
    anchors.top: parent.top
    anchors.margins: Style.space(10)
    spacing: Style.space(5)

    Item {
      width: parent.width
      implicitHeight: Math.max(titleText.implicitHeight, badgeText.implicitHeight)

      Text {
        id: titleText
        anchors.left: parent.left
        anchors.right: badgeText.left
        anchors.rightMargin: Style.space(8)
        anchors.verticalCenter: parent.verticalCenter
        textFormat: Text.PlainText
        text: card.title
        color: card.fg
        font.family: card.fontFamily
        font.pixelSize: Style.font.body
        font.bold: true
        elide: Text.ElideRight
      }
      Text {
        id: badgeText
        anchors.right: parent.right
        anchors.verticalCenter: parent.verticalCenter
        visible: card.badge !== ""
        textFormat: Text.PlainText
        text: card.badge.toUpperCase()
        color: card.fg
        opacity: 0.7
        font.family: card.fontFamily
        font.pixelSize: Style.font.caption
        font.bold: true
        font.letterSpacing: 0.8
      }
    }

    Text {
      width: parent.width
      textFormat: Text.PlainText
      text: card.body
      color: card.fg
      opacity: 0.85
      wrapMode: Text.WordWrap
      font.family: card.fontFamily
      font.pixelSize: Style.font.bodySmall
    }

    Text {
      width: parent.width
      visible: card.refs !== ""
      textFormat: Text.PlainText
      text: card.refs
      color: card.fg
      opacity: 0.5
      wrapMode: Text.WordWrap
      font.family: card.fontFamily
      font.pixelSize: Style.font.caption
      font.italic: true
    }
  }
}
