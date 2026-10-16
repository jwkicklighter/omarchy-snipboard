pragma Singleton
import QtQuick

// Inert theme fixture: no shell imports, processes, config, or desktop access.
QtObject {
    property int cornerRadius: 4
    property color normalFill: "#222222"
    property color focusBorderColor: "#aaaaaa"
    property color selectionFill: "#444444"
    property var font: ({menuFamily: "sans-serif", body: 14, caption: 12})
    function space(value) { return value }
    function controlFill(focused, hovered, foreground, accent) { return normalFill }
    function controlBorder(focused, hovered, foreground, accent) { return focusBorderColor }
    function controlBorderWidth(focused, hovered) { return 1 }
}
