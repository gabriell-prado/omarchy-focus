import QtQuick
import qs.Commons
import qs.Ui
import "../Model.js" as Model

// The brainwave tone: binaural or isochronic beats, their frequency and the
// carrier. The tone is never treated (no width, softness or tape): a binaural
// beat needs each ear's tone exactly as generated.
Column {
  id: page

  property var ctx: null
  readonly property var svc: ctx ? ctx.svc : null
  readonly property var cfg: ctx ? ctx.cfg : ({})
  readonly property var catalog: ctx ? ctx.catalog : null

  width: parent ? parent.width : 0
  spacing: Style.space(10)

  PanelSectionHeader { text: page.ctx.t("tune.tone"); foreground: page.ctx.fg; fontFamily: page.ctx.fontFamily }

  Chips {
    ctx: page.ctx
    options: (page.catalog ? page.catalog.beatModes : []).map(function(m) { return { value: m, label: page.ctx.t("beat." + m) } })
    value: page.cfg.beatMode || ""
    onPicked: function(v) { page.svc.chooseSound({ beatMode: v }) }
  }

  SliderRow {
    ctx: page.ctx
    visible: page.cfg.beatMode !== "off"
    label: page.ctx.t("tune.beat")
    live: true
    value: page.cfg.beatHz || 16
    minimum: 1; maximum: 45
    format: function(v) {
      var hz = Math.round(v)
      return page.ctx.hz(hz) + " · " + page.ctx.t("band." + Model.bandFor(hz, page.catalog ? page.catalog.bands : []))
    }
    onCommitted: function(v) { page.svc.chooseSound({ beatHz: v }) }
  }
  SliderRow {
    ctx: page.ctx
    visible: page.cfg.beatMode !== "off"
    label: page.ctx.t("tune.carrier")
    live: true
    value: page.cfg.carrierHz || 250
    minimum: 100; maximum: 600; step: 10
    format: function(v) { return page.ctx.hz(v) }
    onCommitted: function(v) { page.svc.chooseSound({ carrierHz: v }) }
  }

  Note {
    ctx: page.ctx
    visible: page.cfg.beatMode === "binaural"
    text: Model.GLYPH.headphones + "  " + page.ctx.t("tune.headphones")
    opacity: 0.85
  }
  Note {
    ctx: page.ctx
    visible: page.cfg.beatMode !== "off"
    text: page.ctx.t("tune.carrier.note")
  }
  Note { ctx: page.ctx; text: page.ctx.t("tune.beat.note") }
}
