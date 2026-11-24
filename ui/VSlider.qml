import QtQuick
import qs.Commons
import qs.Commons as Commons
import qs.Ui

// A vertical slider in the kit's PanelSlider look: minimum at the bottom,
// maximum at the top. `moved` fires while dragging, `released` at the end.
Item {
  id: root

  property var ctx: null
  property real value: 0
  property real minimum: 0
  property real maximum: 100
  property real step: 1
  property bool dragging: false
  property real liveValue: value
  readonly property bool hot: mouse.containsMouse || dragging

  signal moved(real value)
  signal released(real value)

  onValueChanged: if (!dragging) liveValue = value

  readonly property color fg: ctx ? ctx.fg : Commons.Color.foreground
  readonly property var bar: ctx ? ctx.bar : null
  property real trackWidth: Math.max(4, Math.round(Style.spacing.controlHeight * 0.11))
  property real knobSize: Math.max(14, Math.round(Style.spacing.controlHeight * 0.38))
  readonly property real range: Math.max(0.0001, maximum - minimum)
  readonly property real progress: Math.max(0, Math.min(1, (liveValue - minimum) / range))

  implicitWidth: knobSize + Style.space(6)
  implicitHeight: Style.space(120)

  Rectangle {
    id: track
    anchors.horizontalCenter: parent.horizontalCenter
    anchors.top: parent.top
    anchors.bottom: parent.bottom
    anchors.topMargin: root.knobSize / 2
    anchors.bottomMargin: root.knobSize / 2
    width: root.trackWidth
    radius: width / 2
    color: root.bar ? Style.selectedFillFor(root.bar.foreground, Commons.Color.accent) : Qt.rgba(root.fg.r, root.fg.g, root.fg.b, 0.15)
  }

  Rectangle {
    anchors.horizontalCenter: track.horizontalCenter
    anchors.bottom: track.bottom
    width: track.width
    radius: track.radius
    color: root.fg
    height: track.height * root.progress
    Behavior on height {
      enabled: !root.dragging
      NumberAnimation { duration: Style.duration(140); easing.type: Easing.OutCubic }
    }
  }

  BorderSurface {
    width: root.knobSize
    height: root.knobSize
    radius: root.knobSize / 2
    color: root.fg
    borderSpec: Border.flat(root.bar ? root.bar.background : "#101315", Math.max(1, Style.space(2)))
    anchors.horizontalCenter: track.horizontalCenter
    y: track.y + track.height * (1 - root.progress) - height / 2
    scale: root.hot ? 1.15 : 1.0

    Behavior on y {
      enabled: !root.dragging
      NumberAnimation { duration: Style.duration(140); easing.type: Easing.OutCubic }
    }
    Behavior on scale {
      NumberAnimation { duration: Style.duration(110); easing.type: Easing.OutCubic }
    }
  }

  MouseArea {
    id: mouse
    anchors.fill: parent
    hoverEnabled: true
    cursorShape: Qt.PointingHandCursor
    // A vertical drag here moves the knob; the panel must not scroll instead.
    preventStealing: true

    function valueFromY(y) {
      if (track.height <= 0) return root.minimum
      var fraction = Math.max(0, Math.min(1, 1 - (y - track.y) / track.height))
      var raw = root.minimum + fraction * root.range
      return Math.max(root.minimum, Math.min(root.maximum, Math.round(raw / root.step) * root.step))
    }

    onPressed: function(event) {
      root.dragging = true
      root.liveValue = valueFromY(event.y)
      root.moved(root.liveValue)
    }
    onPositionChanged: function(event) {
      if (!root.dragging) return
      var next = valueFromY(event.y)
      if (next === root.liveValue) return
      root.liveValue = next
      root.moved(next)
    }
    onReleased: function() {
      root.dragging = false
      root.released(root.liveValue)
      root.liveValue = root.value
    }
    onWheel: function(wheel) {
      var delta = (wheel.angleDelta.y > 0 ? 5 : -5) * root.step
      var next = Math.max(root.minimum, Math.min(root.maximum, root.liveValue + delta))
      root.liveValue = next
      root.moved(next)
      root.released(next)
    }
  }
}
