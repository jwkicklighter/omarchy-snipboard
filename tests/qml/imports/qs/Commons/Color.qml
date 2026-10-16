pragma Singleton
import QtQuick

QtObject {
    property color muted: "#999999"
    property color accent: "#aaaaaa"
    property color urgent: "#a55555"
    property var menu: ({text: "#eeeeee", selectedText: "#ffffff", selectedBackground: "#444444", border: "#777777"})
}
