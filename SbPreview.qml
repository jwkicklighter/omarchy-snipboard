import QtQuick

// Follow the resolved subject, not the selection index: derived bindings may
// still contain the previous row inside an onSelectedChanged handler.
QtObject {
    id: root
    property var subject: null
    property string record: ""
    property int sequence: 0
    property string body: ""
    property string keyword: ""
    property real created: 0
    signal requested(string record)

    onSubjectChanged: load()

    function clear() {
        delay.stop()
        sequence = 0
        record = ""
        body = ""
        keyword = ""
        created = 0
    }

    function load() {
        const next = subject ? subject.id : ""
        if (next && next === record) return
        clear()
        record = next
        if (next) delay.restart()
    }

    function receive(message) {
        if (!sequence || message.seq !== sequence || !subject || subject.id !== record) return
        body = message.body
        keyword = message.keyword || ""
        created = message.created || 0
    }

    property Timer delay: Timer {
        interval: 120
        onTriggered: root.requested(root.record)
    }
}
