import QtQuick
import QtQuick.Controls
import qs.Commons

Button {
    id: control
    font.family: Style.font.menuFamily
    font.pixelSize: Style.font.body
    padding: Style.space(8)
    contentItem: Text {
        text: control.text
        textFormat: Text.PlainText
        font: control.font
        color: control.checked || control.down ? Color.menu.selectedText : Color.menu.text
        horizontalAlignment: Text.AlignHCenter
        verticalAlignment: Text.AlignVCenter
    }
    background: Rectangle {
        radius: Style.cornerRadius
        color: control.checked || control.down ? Color.menu.selectedBackground : Style.controlFill(control.activeFocus, control.hovered, Color.menu.text, Color.accent)
        border.color: Style.controlBorder(control.activeFocus, control.hovered, Color.menu.text, Color.accent)
        border.width: Style.controlBorderWidth(control.activeFocus, control.hovered)
    }
}
