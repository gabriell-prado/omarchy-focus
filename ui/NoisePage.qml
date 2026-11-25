import QtQuick
import qs.Commons
import qs.Ui

// Noise: its color, the ten-band equalizer, and the texture treatments
// (softness and stereo width). The engine applies both to the noise only and
// keeps its loudness; every slider is heard while dragged.
Column {
  id: page

  property var ctx: null
  readonly property var svc: ctx ? ctx.svc : null
  readonly property var cfg: ctx ? ctx.cfg : ({})
  readonly property var catalog: ctx ? ctx.catalog : null

  width: parent ? parent.width : 0
  spacing: Style.space(10)

  PanelSectionHeader { text: page.ctx.t("tune.noise"); foreground: page.ctx.fg; fontFamily: page.ctx.fontFamily }

  Chips {
    ctx: page.ctx
    options: (page.catalog ? page.catalog.noiseColors : []).map(function(c) { return { value: c, label: page.ctx.t("noise." + c) } })
    value: page.cfg.noiseColor || ""
    onPicked: function(v) { page.svc.chooseSound({ noiseColor: v }) }
  }

  Note { ctx: page.ctx; text: page.ctx.t("tune.noise.note") }

  NoiseEqualizer { ctx: page.ctx }

  Note { ctx: page.ctx; text: page.ctx.t("tune.eq.note") }

  PanelSeparator { width: parent.width; foreground: page.ctx.fg }

  PanelSectionHeader { text: page.ctx.t("texture.title"); foreground: page.ctx.fg; fontFamily: page.ctx.fontFamily }

  SliderRow {
    ctx: page.ctx
    label: page.ctx.t("texture.softness")
    live: true
    value: page.cfg.noiseSoftness || 0
    minimum: 0; maximum: 100
    format: function(v) { return Math.round(v) === 0 ? page.ctx.t("texture.off") : page.ctx.percent(v) }
    onCommitted: function(v) { page.svc.chooseSound({ noiseSoftness: v }) }
  }
  Note { ctx: page.ctx; text: page.ctx.t("texture.softness.note") }

  SliderRow {
    ctx: page.ctx
    label: page.ctx.t("texture.width")
    live: true
    value: page.cfg.noiseWidth !== undefined ? page.cfg.noiseWidth : 100
    minimum: 0; maximum: 100
    format: function(v) { return Math.round(v) === 0 ? page.ctx.t("texture.mono") : page.ctx.percent(v) }
    onCommitted: function(v) { page.svc.chooseSound({ noiseWidth: v }) }
  }
  Note { ctx: page.ctx; text: page.ctx.t("texture.width.note") }
}
