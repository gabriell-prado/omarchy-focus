import QtQuick
import qs.Commons
import qs.Commons as Commons

// Everything a page or control needs from its panel, in one object: the
// engine client, the plugin's strings, the bar's colours and font, and the
// formatting helpers. Passed down as `ctx`.
QtObject {
  id: ctx

  property var svc: null
  property var strings: null
  property var bar: null

  readonly property color fg: bar ? bar.foreground : Commons.Color.foreground
  readonly property string fontFamily: bar ? bar.fontFamily : Style.font.family
  readonly property var cfg: svc ? svc.cfg : ({})
  readonly property var catalog: svc ? svc.catalog : null

  function t(key) { return strings ? strings.t(key) : key }
  function tn(key, n) { return strings ? strings.tn(key, n) : key }
  function num(n) { return Commons.I18n.formatNumber(n) }

  // Digits in the language's numbering system (the clock is built in ASCII).
  function digits(text) {
    return String(text).replace(/[0-9]/g, function(d) { return Commons.I18n.formatNumber(Number(d)) })
  }

  function minutes(n) { return tn("unit.minutes", n).arg(num(n)) }
  function percent(n) { return t("unit.percent").arg(num(Math.round(n))) }
  function hz(n) { return t("unit.hz").arg(num(Math.round(n))) }
}
