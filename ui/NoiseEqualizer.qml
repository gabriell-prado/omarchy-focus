import QtQuick
import qs.Commons
import qs.Ui

// Ten octave sliders that shape the noise by hand. While a named color is
// playing they show its shape (from the engine's catalog), so the first move
// switches to a custom noise that starts from exactly what is heard. Every
// step is sent as the knob moves; the engine glides the bands and keeps the
// loudness of the shape the same as pink's.
Column {
  id: eq

  property var ctx: null
  readonly property var cfg: ctx ? ctx.cfg : ({})
  readonly property var catalog: ctx ? ctx.catalog : null
  readonly property var freqs: catalog && catalog.eqBands ? catalog.eqBands : []

  // What the sliders show: the custom shape, or the playing color's shape.
  readonly property var levels: {
    var color = cfg.noiseColor
    var shapes = catalog ? catalog.eqShapes : null
    if (color !== "custom" && shapes && shapes[color]) return shapes[color]
    return cfg.noiseBands || []
  }

  // The band under the pointer or being dragged, for the readout.
  property int activeBand: -1
  property real activeLevel: 0

  width: parent ? parent.width : 0
  spacing: Style.space(6)

  function bandName(hz) {
    return hz < 1000 ? ctx.hz(hz) : ctx.t("unit.khz").arg(ctx.num(hz / 1000))
  }

  // Equalizer labels: 31.5 Hz is written "31", as on any graphic EQ.
  function shortName(hz) {
    return hz < 1000 ? ctx.num(Math.floor(hz)) : ctx.num(hz / 1000) + "k"
  }

  // Send a band's new level once per step, starting from the shown shape.
  function send(index, value) {
    var level = Math.round(value)
    if (cfg.noiseColor === "custom" && levels[index] === level) return
    var next = levels.slice()
    next[index] = level
    ctx.svc.chooseSound({ noiseColor: "custom", noiseBands: next })
  }

  Item {
    width: parent.width
    implicitHeight: Math.max(header.implicitHeight, readout.implicitHeight)

    PanelSectionHeader {
      id: header
      anchors.left: parent.left
      anchors.verticalCenter: parent.verticalCenter
      text: eq.ctx.t("tune.eq")
      foreground: eq.ctx.fg
      fontFamily: eq.ctx.fontFamily
    }
    Text {
      id: readout
      anchors.right: parent.right
      anchors.left: header.right
      anchors.leftMargin: Style.space(8)
      anchors.verticalCenter: parent.verticalCenter
      horizontalAlignment: Text.AlignRight
      textFormat: Text.PlainText
      text: eq.activeBand >= 0
        ? eq.ctx.t("tune.eq.readout").arg(eq.bandName(eq.freqs[eq.activeBand])).arg(eq.ctx.percent(eq.activeLevel))
        : eq.ctx.t("tune.eq.hint")
      color: eq.ctx.fg
      opacity: eq.activeBand >= 0 ? 1 : 0.6
      font.family: eq.ctx.fontFamily
      font.pixelSize: Style.font.caption
      font.bold: eq.activeBand >= 0
      elide: Text.ElideLeft
    }
  }

  Row {
    id: sliders
    width: parent.width
    spacing: Style.space(2)
    readonly property real cell: eq.freqs.length > 0 ? (width - spacing * (eq.freqs.length - 1)) / eq.freqs.length : 0

    Repeater {
      model: eq.freqs.length

      Column {
        id: band
        required property int index
        width: sliders.cell
        spacing: Style.space(4)

        VSlider {
          id: slider
          anchors.horizontalCenter: parent.horizontalCenter
          ctx: eq.ctx
          height: Style.space(112)
          value: eq.levels[band.index] !== undefined ? eq.levels[band.index] : 100
          // Shown, but quieter, while the noise is off.
          opacity: eq.cfg.noiseColor === "off" ? 0.45 : 1
          onHotChanged: {
            if (hot) {
              eq.activeBand = band.index
              eq.activeLevel = liveValue
            } else if (eq.activeBand === band.index) {
              eq.activeBand = -1
            }
          }
          onLiveValueChanged: if (eq.activeBand === band.index) eq.activeLevel = liveValue
          onMoved: function(v) { eq.send(band.index, v) }
          onReleased: function(v) { eq.send(band.index, v) }
        }

        Text {
          anchors.horizontalCenter: parent.horizontalCenter
          textFormat: Text.PlainText
          text: eq.shortName(eq.freqs[band.index])
          color: eq.ctx.fg
          opacity: eq.activeBand === band.index ? 1 : 0.6
          font.family: eq.ctx.fontFamily
          font.pixelSize: Style.font.caption
          font.bold: eq.activeBand === band.index
        }
      }
    }
  }
}
