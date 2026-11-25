import QtQuick
import qs.Commons
import qs.Ui
import "../Model.js" as Model

// Everything about the focus sound on one page, kept short: the switch and
// research-based presets (with their evidence), volumes, the noise (color,
// equalizer, softness, width) and the brainwave tone. The engine applies
// every change while sliders are dragged.
Column {
  id: page

  property var ctx: null
  readonly property var svc: ctx ? ctx.svc : null
  readonly property var cfg: ctx ? ctx.cfg : ({})
  readonly property var catalog: ctx ? ctx.catalog : null
  readonly property var presets: catalog ? catalog.soundPresets : []
  readonly property string current: svc ? svc.soundPreset : "custom"
  readonly property var preset: Model.find(presets, current)
  readonly property bool toneOn: cfg.beatMode !== undefined && cfg.beatMode !== "off"

  readonly property string status: {
    if (!svc) return ""
    if (svc.soundFailed) return ctx.t("sound.status.failed")
    if (svc.soundPlaying && !svc.soundWanted) return ctx.t("sound.status.preview")
    if (svc.soundPlaying) return ctx.t("sound.status.playing")
    if (!cfg.soundEnabled) return ctx.t("sound.status.off")
    return cfg.soundOnlyDuringFocus ? ctx.t("sound.status.waiting") : ctx.t("sound.status.starting")
  }

  // "16 Hz · Beta": the band's name without its description.
  function beatText(v) {
    var hz = Math.round(v)
    var band = ctx.t("band." + Model.bandFor(hz, catalog ? catalog.bands : [])).split(" · ")[0]
    return ctx.hz(hz) + " · " + band
  }

  width: parent ? parent.width : 0
  spacing: Style.space(8)

  // ================= Sound =================
  Item {
    width: parent.width
    implicitHeight: Math.max(title.implicitHeight, master.implicitHeight)

    Column {
      id: title
      anchors.left: parent.left
      anchors.right: master.left
      anchors.rightMargin: Style.space(10)
      anchors.verticalCenter: parent.verticalCenter
      spacing: Style.space(2)

      Text {
        width: parent.width
        textFormat: Text.PlainText
        text: page.ctx.t("sound.title")
        color: page.ctx.fg
        font.family: page.ctx.fontFamily
        font.pixelSize: Style.font.title
        font.bold: true
        elide: Text.ElideRight
      }
      Text {
        width: parent.width
        textFormat: Text.PlainText
        text: page.status
        color: page.ctx.fg
        opacity: 0.6
        font.family: page.ctx.fontFamily
        font.pixelSize: Style.font.caption
        elide: Text.ElideRight
      }
    }

    ToggleSwitch {
      id: master
      anchors.right: parent.right
      anchors.verticalCenter: parent.verticalCenter
      foreground: page.ctx.fg
      checked: !!page.cfg.soundEnabled
      onToggled: page.svc.setSoundEnabled(!page.cfg.soundEnabled)
    }
  }

  Grid {
    id: presetGrid
    width: parent.width
    columns: 3
    spacing: Style.space(4)
    readonly property real cell: (width - spacing * (columns - 1)) / columns

    Repeater {
      model: page.presets
      PanelButton {
        required property var modelData
        ctx: page.ctx
        width: presetGrid.cell
        leftAlign: true
        verticalPadding: Style.spacing.controlPaddingY
        iconText: modelData.beatMode === "off" ? Model.GLYPH.wave : Model.GLYPH.headphones
        text: page.ctx.t("preset." + modelData.id + ".name")
        active: page.current === modelData.id
        onClicked: page.svc.applySoundPreset(modelData.id)
      }
    }
  }

  InfoCard {
    ctx: page.ctx
    title: page.preset ? page.ctx.t("preset." + page.preset.id + ".name") : page.ctx.t("sound.custom")
    badge: page.preset ? page.ctx.t("evidence." + page.preset.evidence) : ""
    body: page.preset ? page.ctx.t("preset." + page.preset.id + ".note") : page.ctx.t("sound.custom.note")
    refs: page.preset ? page.preset.refs : ""
  }

  Row {
    id: volumes
    width: parent.width
    spacing: Style.space(14)
    readonly property real half: (width - spacing) / 2

    SliderRow {
      ctx: page.ctx
      width: page.toneOn ? volumes.half : volumes.width
      visible: page.cfg.noiseColor !== "off"
      label: page.ctx.t("sound.noiseVolume")
      live: true
      value: page.cfg.noiseVolume || 0
      minimum: 0; maximum: 100
      format: function(v) { return page.ctx.percent(v) }
      onCommitted: function(v) { page.svc.chooseSound({ noiseVolume: v }) }
    }
    SliderRow {
      ctx: page.ctx
      width: page.cfg.noiseColor !== "off" ? volumes.half : volumes.width
      visible: page.toneOn
      label: page.ctx.t("sound.toneVolume")
      live: true
      value: page.cfg.toneVolume || 0
      minimum: 0; maximum: 100
      format: function(v) { return page.ctx.percent(v) }
      onCommitted: function(v) { page.svc.chooseSound({ toneVolume: v }) }
    }
  }

  SwitchRow {
    ctx: page.ctx
    label: page.ctx.t("sound.onlyFocus")
    checked: !!page.cfg.soundOnlyDuringFocus
    onToggled: page.svc.update({ soundOnlyDuringFocus: !page.cfg.soundOnlyDuringFocus })
  }

  PanelSeparator { width: parent.width; foreground: page.ctx.fg }

  // ================= Noise =================
  PanelSectionHeader { text: page.ctx.t("tune.noise"); foreground: page.ctx.fg; fontFamily: page.ctx.fontFamily }

  Chips {
    ctx: page.ctx
    options: (page.catalog ? page.catalog.noiseColors : []).map(function(c) { return { value: c, label: page.ctx.t("noise." + c) } })
    value: page.cfg.noiseColor || ""
    onPicked: function(v) { page.svc.chooseSound({ noiseColor: v }) }
  }

  NoiseEqualizer { ctx: page.ctx; sliderHeight: Style.space(88) }

  Row {
    id: texture
    width: parent.width
    spacing: Style.space(14)
    readonly property real half: (width - spacing) / 2

    SliderRow {
      ctx: page.ctx
      width: texture.half
      label: page.ctx.t("texture.softness")
      live: true
      value: page.cfg.noiseSoftness || 0
      minimum: 0; maximum: 100
      format: function(v) { return Math.round(v) === 0 ? page.ctx.t("texture.off") : page.ctx.percent(v) }
      onCommitted: function(v) { page.svc.chooseSound({ noiseSoftness: v }) }
    }
    SliderRow {
      ctx: page.ctx
      width: texture.half
      label: page.ctx.t("texture.width")
      live: true
      value: page.cfg.noiseWidth !== undefined ? page.cfg.noiseWidth : 100
      minimum: 0; maximum: 100
      format: function(v) { return Math.round(v) === 0 ? page.ctx.t("texture.mono") : page.ctx.percent(v) }
      onCommitted: function(v) { page.svc.chooseSound({ noiseWidth: v }) }
    }
  }

  Note { ctx: page.ctx; text: page.ctx.t("noise.note") }

  PanelSeparator { width: parent.width; foreground: page.ctx.fg }

  // ================= Brainwave tone =================
  PanelSectionHeader { text: page.ctx.t("tune.tone"); foreground: page.ctx.fg; fontFamily: page.ctx.fontFamily }

  Chips {
    ctx: page.ctx
    options: (page.catalog ? page.catalog.beatModes : []).map(function(m) { return { value: m, label: page.ctx.t("beat." + m) } })
    value: page.cfg.beatMode || ""
    onPicked: function(v) { page.svc.chooseSound({ beatMode: v }) }
  }

  Row {
    id: tone
    visible: page.toneOn
    width: parent.width
    spacing: Style.space(14)
    readonly property real half: (width - spacing) / 2

    SliderRow {
      ctx: page.ctx
      width: tone.half
      label: page.ctx.t("tune.beat")
      live: true
      value: page.cfg.beatHz || 16
      minimum: 1; maximum: 45
      format: function(v) { return page.beatText(v) }
      onCommitted: function(v) { page.svc.chooseSound({ beatHz: v }) }
    }
    SliderRow {
      ctx: page.ctx
      width: tone.half
      label: page.ctx.t("tune.carrier")
      live: true
      value: page.cfg.carrierHz || 250
      minimum: 100; maximum: 600; step: 10
      format: function(v) { return page.ctx.hz(v) }
      onCommitted: function(v) { page.svc.chooseSound({ carrierHz: v }) }
    }
  }

  Note {
    ctx: page.ctx
    visible: page.toneOn
    text: (page.cfg.beatMode === "binaural" ? Model.GLYPH.headphones + "  " : "") + page.ctx.t(page.cfg.beatMode === "binaural" ? "tone.note.binaural" : "tone.note")
  }
}
