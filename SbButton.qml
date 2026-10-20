import QtQuick
import QtQuick.Controls
import qs.Commons
import qs.Commons as Commons

Button {
    id: control
    // Optional Nerd Font glyph drawn before the label.
    property string glyph: ""
    // Destructive actions take the urgent palette role.
    property bool danger: false
    // Emphasized default action (e.g. Save).
    property bool primary: false
    // Tooltip, and the accessible name for glyph-only buttons.
    property string hint: ""
    readonly property color tone: danger ? Commons.Color.urgent : Commons.Color.accent
    readonly property bool lit: checked || down || primary
    font.family: Style.font.menuFamily
    font.pixelSize: Style.font.body
    leftPadding: Style.space(10)
    rightPadding: Style.space(10)
    topPadding: Style.space(5)
    bottomPadding: Style.space(5)
    implicitHeight: Math.max(Style.space(28), implicitContentHeight + topPadding + bottomPadding)
    focusPolicy: Qt.TabFocus
    opacity: enabled ? 1 : 0.4
    Accessible.name: text || hint
    ToolTip.visible: hint.length > 0 && hovered
    ToolTip.delay: 500
    ToolTip.text: hint
    HoverHandler { cursorShape: control.enabled ? Qt.PointingHandCursor : Qt.ArrowCursor }
    contentItem: Row {
        spacing: control.glyph.length && control.text.length ? Style.space(6) : 0
        Text {
            visible: control.glyph.length > 0
            anchors.verticalCenter: parent.verticalCenter
            text: control.glyph
            textFormat: Text.PlainText
            font.family: control.font.family
            font.pixelSize: Style.font.subtitle
            color: label.color
        }
        Text {
            id: label
            anchors.verticalCenter: parent.verticalCenter
            text: control.text
            textFormat: Text.PlainText
            font: control.font
            color: control.danger ? Commons.Color.urgent : control.lit ? Commons.Color.menu.selectedText : Commons.Color.menu.text
        }
    }
    background: Rectangle {
        radius: Style.cornerRadius
        color: control.lit ? Commons.Color.menu.selectedBackground : Style.controlFill(control.visualFocus, control.hovered, Commons.Color.menu.text, control.tone)
        border.color: control.lit ? control.tone : Style.controlBorder(control.visualFocus, control.hovered, control.danger ? Commons.Color.urgent : Commons.Color.menu.text, control.tone)
        border.width: control.lit ? 1 : Style.controlBorderWidth(control.visualFocus, control.hovered)
    }
}
