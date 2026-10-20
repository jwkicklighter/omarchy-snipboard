import QtQuick

// Let the picker window unmap before asking the worker to send a paste shortcut.
QtObject {
    id: root
    property var pending: null
    readonly property bool waiting: pending !== null
    signal hideRequested()
    signal pasteRequested(string record)

    function start(record) {
        if (waiting || !record) return
        pending = {record: record}
        hideRequested()
        delay.start()
    }

    function cancel() {
        delay.stop()
        pending = null
    }

    property Timer delay: Timer {
        interval: 150
        onTriggered: {
            const request = root.pending
            root.pending = null
            if (request) root.pasteRequested(request.record)
        }
    }
}
