import QtQuick
import qs.Commons
import qs.Commons as Commons
import qs.Ui

// The kit's Button in this panel's colours and sizes.
Button {
  property var ctx: null

  foreground: ctx ? ctx.fg : Commons.Color.foreground
  fontFamily: ctx ? ctx.fontFamily : Style.font.family
  fontSize: Style.font.bodySmall
  iconSize: Style.font.title
  bordered: true
  horizontalPadding: Style.spacing.controlPaddingX
  verticalPadding: Style.spacing.controlPaddingY + Style.space(2)
}
