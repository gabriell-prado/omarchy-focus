import QtQuick
import qs.Commons
import qs.Ui
import "../Model.js" as Model

Column {
  id: page

  property var ctx: null
  readonly property var svc: ctx ? ctx.svc : null
  readonly property var cfg: ctx ? ctx.cfg : ({})
  readonly property var presets: ctx && ctx.catalog ? ctx.catalog.soundPresets : []
  readonly property string current: svc ? svc.soundPreset : "custom"
  readonly property var preset: Model.find(presets, current)

  readonly property string status: {
    if (!svc) return ""
    if (svc.soundFailed) return ctx.t("sound.status.failed")
    if (svc.soundPlaying && !svc.soundWanted) return ctx.t("sound.status.preview")
    if (svc.soundPlaying) return ctx.t("sound.status.playing")
    if (!cfg.soundEnabled) return ctx.t("sound.status.off")
    return cfg.soundOnlyDuringFocus ? ctx.t("sound.status.waiting") : ctx.t("sound.status.starting")
  }

  width: parent ? parent.width : 0
  spacing: Style.space(10)

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

  PanelSectionHeader { text: page.ctx.t("sound.presets"); foreground: page.ctx.fg; fontFamily: page.ctx.fontFamily }

  Column {
    width: parent.width
    spacing: Style.space(4)

    Repeater {
      model: page.presets

      PanelButton {
        id: presetRow
        required property var modelData
        ctx: page.ctx
        width: parent.width
        leftAlign: true
        iconText: modelData.beatMode === "off" ? Model.GLYPH.wave : Model.GLYPH.headphones
        text: page.ctx.t("preset." + modelData.id + ".name")
        active: page.current === modelData.id
        onClicked: page.svc.applySoundPreset(modelData.id)

        Text {
          anchors.right: parent.right
          anchors.rightMargin: Style.spacing.controlPaddingX
          anchors.verticalCenter: parent.verticalCenter
          textFormat: Text.PlainText
          text: page.ctx.t("evidence." + presetRow.modelData.evidence + ".short")
          color: page.ctx.fg
          opacity: 0.55
          font.family: page.ctx.fontFamily
          font.pixelSize: Style.font.caption
        }
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

  Note {
    ctx: page.ctx
    visible: page.cfg.beatMode === "binaural"
    text: Model.GLYPH.headphones + "  " + page.ctx.t("tune.headphones")
    opacity: 0.85
  }

  SliderRow {
    ctx: page.ctx
    visible: page.cfg.noiseColor !== "off"
    label: page.ctx.t("sound.noiseVolume")
    value: page.cfg.noiseVolume || 0
    minimum: 0; maximum: 100; step: 5
    format: function(v) { return page.ctx.percent(v) }
    onCommitted: function(v) { page.svc.chooseSound({ noiseVolume: v }) }
  }
  SliderRow {
    ctx: page.ctx
    visible: page.cfg.beatMode !== "off"
    label: page.ctx.t("sound.toneVolume")
    value: page.cfg.toneVolume || 0
    minimum: 0; maximum: 100; step: 5
    format: function(v) { return page.ctx.percent(v) }
    onCommitted: function(v) { page.svc.chooseSound({ toneVolume: v }) }
  }

  SwitchRow {
    ctx: page.ctx
    label: page.ctx.t("sound.onlyFocus")
    checked: !!page.cfg.soundOnlyDuringFocus
    onToggled: page.svc.update({ soundOnlyDuringFocus: !page.cfg.soundOnlyDuringFocus })
  }

  Note { ctx: page.ctx; text: page.ctx.t("sound.volume.note") }
}
