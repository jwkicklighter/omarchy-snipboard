pragma ComponentBehavior: Bound
import QtQuick
import QtQuick.Controls
import QtQuick.Layouts
import QtQuick.Window
import Quickshell
import Quickshell.Io
import Quickshell.Wayland
import Quickshell.Hyprland
import qs.Commons

Item {
    id: root
    property string omarchyPath: Quickshell.env("OMARCHY_PATH")
    property var manifest
    property bool opened: false
    property bool ready: false
    property string status: "Starting isolated synthetic-data helper…"
    property string kind: "all"
    property int sequence: 0
    property int wantedSearch: 0
    property int runningSearch: 0
    property int previewSequence: 0
    property string previewRecord: ""
    property var rows: []
    property int selected: 0
    property string preview: ""
    property bool scratchVisible: false
    property bool expansionEnabled: false
    property bool injecting: false
    property int scratchRevision: 0
    property int scratchFloor: 0
    property bool typedScratch: false
    property var pendingScratch: null
    property var pendingRender: null
    property real inputAt: 0
    property real openedAt: 0
    property var modelLatency: []
    property var openLatency: []
    property real matchMs: 0
    property int assignedSequence: 0
    property int pendingFrameSequence: 0
    property real pendingFrameInputAt: 0
    property var frameLatency: []
    property var openFrameLatency: []
    property var testResults: []
    property string testPhase: "idle"
    property int testIndex: 0
    property int testCharacter: 0
    property int testWait: 0
    property bool benchmarkRunning: false
    property int benchmarkIndex: 0
    property int layoutResets: 0
    property string layoutProbeResult: "not run"
    readonly property string sourceDirectory: decodeURIComponent(Qt.resolvedUrl(".").toString().replace(/^file:\/\//, "").replace(/\/$/, ""))
    readonly property var activeRow: rows.length > selected ? rows[selected] : null

    function send(operation, fields) {
        if (!ready) return 0
        let request = fields || {}
        request.op = operation
        request.seq = ++sequence
        helper.write(JSON.stringify(request) + "\n")
        return request.seq
    }

    function open(payloadJson) {
        openedAt = Date.now()
        opened = true
        expansionEnabled = false
        scratchVisible = false
        resetScratch()
        search.text = ""
        kind = "all"
        requestSearch()
        Qt.callLater(function() { search.forceActiveFocus() })
    }

    function close() {
        opened = false
        expansionEnabled = false
        resetScratch()
        pendingRender = null
        previewTimer.stop()
    }

    function requestSearch() {
        wantedSearch = ++sequence
        inputAt = Date.now()
        if (runningSearch === 0) startSearch()
    }

    function startSearch() {
        if (!ready) return
        runningSearch = wantedSearch
        helper.write(JSON.stringify({op: "search", seq: runningSearch, query: search.text, kind: kind}) + "\n")
    }

    function select(delta) {
        if (!rows.length) return
        selected = (selected + delta + rows.length) % rows.length
        results.positionViewAtIndex(selected, ListView.Contain)
    }

    function loadPreview() {
        if (activeRow && activeRow.id === previewRecord) return
        previewTimer.restart()
    }

    function copyOnly() {
        if (activeRow) send("copy", {record: activeRow.id, clipboard: clipboardInput.text})
    }

    function paste() {
        // No generic sendshortcut/wtype fallback: target mode cannot be verified.
        status = "Paste blocked: external input context is unverified. Use Copy only, or the disposable scratch target."
    }

    function resetScratch() {
        scratchRevision++
        scratchFloor = scratch.cursorPosition
        typedScratch = false
        pendingScratch = null
    }

    function inspectScratch() {
        if (!typedScratch || injecting || !opened || !expansionEnabled || !scratch.activeFocus || scratch.selectedText.length) return
        typedScratch = false
        const position = scratch.cursorPosition
        const start = Math.max(0, position - 65)
        if (position < scratchFloor || position - start < 1) return
        const revision = scratchRevision
        const seq = send("scratch", {enabled: true, context: "trusted-scratch", prefix: scratch.text.slice(start, position), clipboard: clipboardInput.text})
        pendingScratch = {seq: seq, revision: revision, position: position, floor: start}
    }

    function insertScratch() {
        if (!activeRow) return
        resetScratch()
        const seq = send("render", {record: activeRow.id, clipboard: clipboardInput.text})
        pendingRender = {seq: seq, record: activeRow.id, revision: scratchRevision}
    }

    function receive(message) {
        if (message.op === "ready") {
            ready = true
            status = message.count + " synthetic records · no existing history accessed · external paste/expansion blocked"
            requestSearch()
            return
        }
        if (message.error) { status = message.error; return }
        if (message.op === "search") {
            runningSearch = 0
            if (message.seq === wantedSearch) {
                rows = message.rows
                // Keep delegates and unchanged roles across query updates.
                for (let i = 0; i < rows.length; i++) {
                    const row = rows[i]
                    const value = {recordId: row.id, name: row.name, kind: row.kind, summary: row.summary}
                    if (i >= displayModel.count) displayModel.append(value)
                    else if (displayModel.get(i).recordId !== row.id) displayModel.set(i, value)
                }
                if (displayModel.count > rows.length) displayModel.remove(rows.length, displayModel.count - rows.length)
                selected = 0
                matchMs = message.matchMs
                assignedSequence = message.seq
                pendingFrameSequence = message.seq
                pendingFrameInputAt = inputAt
                // Model assignment timing only, NOT presented/visible frame latency.
                modelLatency = modelLatency.concat([Date.now() - inputAt]).slice(-1000)
                loadPreview()
                if (openedAt && opened) openLatency = openLatency.concat([Date.now() - openedAt]).slice(-1000)
            } else startSearch() // Coalesce rapid input: at most one stale query runs.
        } else if (message.op === "preview" && message.seq === previewSequence && activeRow && activeRow.id === previewRecord) {
            preview = message.body
        } else if (message.op === "copy") {
            status = message.message
        } else if (message.op === "scratch") {
            const pending = pendingScratch
            if (!pending || message.seq !== pending.seq) return
            pendingScratch = null
            const expansion = message.expansion
            if (!expansion || !opened || !expansionEnabled || !scratch.activeFocus || scratchRevision !== pending.revision || scratch.cursorPosition !== pending.position) return
            if (pending.position - expansion.remove < scratchFloor) return
            injecting = true
            const start = pending.position - expansion.remove
            scratch.remove(start, pending.position)
            scratch.insert(start, expansion.text)
            scratch.cursorPosition = start + expansion.cursor
            injecting = false
            resetScratch()
            send("used", {record: expansion.record})
            status = "Expanded inside disposable TextEdit only; no external input injected."
        } else if (message.op === "render") {
            const pending = pendingRender
            if (!pending || pending.seq !== message.seq || !opened || !scratchVisible || pending.revision !== scratchRevision) return
            pendingRender = null
            injecting = true
            scratch.text = message.text
            scratch.cursorPosition = message.cursor
            injecting = false
            scratch.forceActiveFocus()
            resetScratch()
            send("used", {record: pending.record})
            status = "Inserted inside disposable TextEdit only."
        }
    }

    // Diagnostic aggregates contain no queries, input buffers, IDs, or payloads.
    function metrics(ignored) {
        return JSON.stringify({matchingLastMs: matchMs, inputToModelAssignmentMs: modelLatency,
                               openToModelAssignmentMs: openLatency, syntheticInputToFrameSwappedMs: frameLatency,
                               openToFrameSwappedMs: openFrameLatency, physicalPresentationMeasured: false,
                               rows: rows.length, ready: ready, searchFocused: search.activeFocus,
                               benchmarkRunning: benchmarkRunning, testPhase: testPhase, tests: testResults,
                               layoutResets: layoutResets, layoutProbeResult: layoutProbeResult})
    }

    function frameSwapped() {
        if (!opened || !ready) return
        if (pendingFrameSequence && pendingFrameSequence === assignedSequence && pendingFrameSequence === wantedSearch) {
            frameLatency = frameLatency.concat([Date.now() - pendingFrameInputAt]).slice(-1000)
            pendingFrameSequence = 0
        }
        if (openedAt && rows.length && search.activeFocus && runningSearch === 0) {
            openFrameLatency = openFrameLatency.concat([Date.now() - openedAt]).slice(-1000)
            openedAt = 0
        }
    }

    // Explicit developer-only IPC probes. They edit only this disposable field,
    // never synthesize OS input, access other apps, or change the clipboard.
    function selfTest(ignored) {
        if (!opened || !ready) return "not ready"
        testResults = []
        testIndex = 0
        testCharacter = 0
        testWait = 0
        testPhase = "prepare"
        scratchVisible = true
        expansionEnabled = true
        clipboardInput.text = "literal {date}"
        scratchTestTimer.start()
        return "started (simulated TextEdit input, not hardware keyboard events)"
    }

    function contextProbe(action) {
        if (!opened || !ready) return "not ready"
        if (action === "prime") {
            scratchVisible = true
            expansionEnabled = true
            injecting = true
            scratch.text = "!em"
            scratch.cursorPosition = 3
            injecting = false
            scratch.forceActiveFocus()
            resetScratch()
            scratchFloor = 0
            layoutProbeResult = "primed"
        } else if (action === "finish") {
            typedScratch = true
            scratch.insert(scratch.cursorPosition, "ail")
            layoutProbeTimer.start()
        } else return "unsupported"
        return "ok"
    }

    function benchmark(ignored) {
        if (!opened || !ready) return "not ready"
        expansionEnabled = false
        scratchVisible = false
        frameLatency = []
        modelLatency = []
        openFrameLatency = []
        benchmarkIndex = 0
        benchmarkRunning = true
        benchmarkTimer.start()
        return "started (synthetic field changes to frameSwapped proxy)"
    }

    onSelectedChanged: loadPreview()
    onExpansionEnabledChanged: resetScratch()
    Keys.onEscapePressed: function(event) { root.close(); event.accepted = true }
    Component.onCompleted: helper.running = true
    Component.onDestruction: {
        expansionEnabled = false
        helper.running = false
    }

    Connections {
        target: Hyprland
        function onRawEvent(event) {
            // Never read active-window event data (it may contain private titles).
            if (event.name === "activelayout") { root.layoutResets++; root.resetScratch() }
            else if (event.name === "activewindow" || event.name === "activewindowv2" || event.name === "lockgroups") root.resetScratch()
        }
    }

    Connections {
        target: card.Window.window
        function onFrameSwapped() { root.frameSwapped() }
    }

    Timer {
        id: layoutProbeTimer
        interval: 500
        onTriggered: {
            root.layoutProbeResult = scratch.text === "!email" ? "stale trigger refused" : "FAILED"
            root.expansionEnabled = false
        }
    }

    Timer {
        id: scratchTestTimer
        interval: 40
        repeat: true
        onTriggered: {
            const keywords = ["!email", "!unicode", "!multi", "!vars"]
            if (!root.opened || !root.ready) { stop(); root.testPhase = "aborted"; return }
            if (root.testIndex >= keywords.length) {
                stop(); root.expansionEnabled = false; root.testPhase = "complete"; return
            }
            if (root.testPhase === "prepare") {
                root.injecting = true
                scratch.text = ""
                scratch.cursorPosition = 0
                root.injecting = false
                scratch.forceActiveFocus()
                root.resetScratch()
                root.testPhase = "typing"
                root.testCharacter = 0
            } else if (root.testPhase === "typing") {
                const keyword = keywords[root.testIndex]
                root.typedScratch = true
                scratch.insert(scratch.cursorPosition, keyword[root.testCharacter++])
                if (root.testCharacter === keyword.length) { root.testPhase = "waiting"; root.testWait = 0 }
            } else if (++root.testWait >= 10) {
                const expected = ["scratch@example.invalid", "Hello — Καλημέρα 🌍", "First line\nSecond line\nThird line"]
                const valid = root.testIndex < 3 ? scratch.text === expected[root.testIndex] :
                    /^\d{4}-\d{2}-\d{2} \d{2}:\d{2}\nliteral \{date\}\nBefore after$/.test(scratch.text) && scratch.cursorPosition === scratch.text.indexOf("after")
                root.testResults = root.testResults.concat([{case: root.testIndex, passed: valid}])
                root.testIndex++
                root.testPhase = "prepare"
            }
        }
    }

    Timer {
        id: benchmarkTimer
        interval: 90
        repeat: true
        onTriggered: {
            if (!root.opened || !root.ready) { stop(); root.benchmarkRunning = false; return }
            const queries = ["e", "em", "ema", "email", "!eml", "prjnt", "project", "amber", "river", "999", "καλη", "zzzz", ""]
            if (root.benchmarkIndex >= 260) { stop(); root.benchmarkRunning = false; return }
            if (root.benchmarkIndex % 13 === 0) {
                root.close()
                root.open("{}")
            }
            search.text = queries[root.benchmarkIndex++ % queries.length]
            search.forceActiveFocus()
        }
    }

    Process {
        id: helper
        command: ["setpriv", "--pdeathsig", "TERM", "python", "-m", "snipboard.worker"]
        workingDirectory: root.sourceDirectory
        stdinEnabled: true
        stdout: SplitParser {
            onRead: function(line) {
                try { root.receive(JSON.parse(line)) }
                catch (error) { root.status = "Invalid helper response; private details suppressed." }
            }
        }
        stderr: SplitParser { onRead: function(line) { root.status = "Helper diagnostic suppressed; run automated tests for details." } }
        onExited: {
            root.ready = false
            root.runningSearch = 0
            root.status = "Helper stopped. Invalid storage is preserved; rebuild/reload after resolving errors."
        }
    }

    ListModel { id: displayModel }
    Timer {
        id: previewTimer
        interval: 120
        onTriggered: {
            if (!root.activeRow) { root.preview = ""; root.previewRecord = ""; return }
            root.previewRecord = root.activeRow.id
            root.previewSequence = root.send("preview", {record: root.previewRecord})
        }
    }

    PanelWindow {
        id: panel
        visible: root.opened
        anchors { top: true; bottom: true; left: true; right: true }
        color: "transparent"
        exclusionMode: ExclusionMode.Ignore
        WlrLayershell.namespace: "omarchy-snipboard"
        WlrLayershell.layer: WlrLayer.Overlay
        WlrLayershell.keyboardFocus: WlrKeyboardFocus.Exclusive

        Rectangle { anchors.fill: parent; color: Color.menu.scrim }
        MouseArea { anchors.fill: parent; onClicked: root.close() }

        Rectangle {
            id: card
            anchors.centerIn: parent
            width: Math.min(Style.space(900), panel.width - Style.space(32))
            height: Math.min(Style.space(580), panel.height - Style.space(32))
            radius: Style.cornerRadius
            color: Color.menu.background
            border.color: Color.menu.border
            border.width: Math.max(1, Style.normalBorderWidth)
            MouseArea { anchors.fill: parent }

            ColumnLayout {
                anchors.fill: parent
                anchors.margins: Style.space(16)
                spacing: Style.space(10)

                RowLayout {
                    Layout.fillWidth: true
                    Text { text: "Snipboard · synthetic prototype"; color: Color.menu.text; font.family: Style.font.menuFamily; font.pixelSize: Style.font.title }
                    Item { Layout.fillWidth: true }
                    SbButton { text: "Scratch"; checkable: true; checked: root.scratchVisible; onClicked: { root.scratchVisible = !root.scratchVisible; root.resetScratch() } }
                    SbButton { text: "Close"; onClicked: root.close() }
                }

                TextField {
                    id: search
                    Layout.fillWidth: true
                    placeholderText: "Search clipboard and snippets…"
                    maximumLength: 256
                    color: Color.menu.text
                    placeholderTextColor: Color.muted
                    selectionColor: Style.selectionFill
                    selectedTextColor: Color.menu.selectedText
                    font.family: Style.font.menuFamily
                    font.pixelSize: Style.font.heading
                    background: Rectangle { color: Style.normalFill; border.color: search.activeFocus ? Style.focusBorderColor : Color.menu.border; border.width: 1; radius: Style.cornerRadius }
                    onTextChanged: root.requestSearch()
                    Keys.priority: Keys.BeforeItem
                    Keys.onPressed: function(event) {
                        if (event.key === Qt.Key_Escape) {
                            if (text.length) clear(); else root.close()
                        } else if (event.key === Qt.Key_Down) root.select(1)
                        else if (event.key === Qt.Key_Up) root.select(-1)
                        else if (event.key === Qt.Key_PageDown) root.select(6)
                        else if (event.key === Qt.Key_PageUp) root.select(-6)
                        else if (event.key === Qt.Key_Return || event.key === Qt.Key_Enter) root.paste()
                        else if (event.key === Qt.Key_C && (event.modifiers & (Qt.ControlModifier | Qt.MetaModifier))) {
                            if (selectedText.length) { copy(); event.accepted = true; return }
                            root.copyOnly()
                        } else return // Preserve editing, selection, Home/End, IME, and paste.
                        event.accepted = true
                    }
                }

                RowLayout {
                    Repeater {
                        model: [{label: "All", value: "all"}, {label: "Clipboard", value: "clipboard"}, {label: "Snippets", value: "snippet"}]
                        SbButton {
                            required property var modelData
                            text: modelData.label
                            checked: root.kind === modelData.value
                            onClicked: { root.kind = modelData.value; root.requestSearch(); search.forceActiveFocus() }
                        }
                    }
                    Item { Layout.fillWidth: true }
                    SbButton { text: "Copy only"; enabled: root.activeRow !== null; onClicked: root.copyOnly() }
                }

                RowLayout {
                    Layout.fillWidth: true
                    Layout.fillHeight: true
                    spacing: Style.space(16)
                    ListView {
                        id: results
                        Layout.fillWidth: true
                        Layout.fillHeight: true
                        Layout.preferredWidth: card.width / 2
                        clip: true
                        model: displayModel
                        boundsBehavior: Flickable.StopAtBounds
                        spacing: Style.space(4)
                        delegate: Rectangle {
                            id: row
                            required property string recordId
                            required property string name
                            required property string kind
                            required property string summary
                            required property int index
                            width: results.width
                            height: Style.space(62)
                            radius: Style.cornerRadius
                            color: index === root.selected ? Color.menu.selectedBackground : "transparent"
                            Column {
                                anchors.fill: parent
                                anchors.margins: Style.space(8)
                                Text { width: parent.width; text: (row.kind === "snippet" ? "Snippet · " : "Clipboard · ") + row.name; textFormat: Text.PlainText; color: row.index === root.selected ? Color.menu.selectedText : Color.menu.text; font.family: Style.font.menuFamily; font.pixelSize: Style.font.body; elide: Text.ElideRight }
                                Text { width: parent.width; text: row.summary; textFormat: Text.PlainText; color: Color.menu.text; opacity: 0.7; font.family: Style.font.menuFamily; font.pixelSize: Style.font.caption; elide: Text.ElideRight }
                            }
                            MouseArea {
                                anchors.fill: parent
                                hoverEnabled: true
                                onPositionChanged: root.selected = row.index
                                onClicked: { root.selected = row.index; root.paste() }
                            }
                        }
                        Text { anchors.centerIn: parent; visible: !root.rows.length; text: root.ready ? "No matches" : "Loading…"; color: Color.menu.text; font.family: Style.font.menuFamily }
                    }

                    ColumnLayout {
                        Layout.fillWidth: true
                        Layout.fillHeight: true
                        Layout.preferredWidth: card.width / 2
                        Text { text: root.scratchVisible ? "Disposable input · never an external app" : "Preview · exact stored text"; color: Color.menu.text; font.family: Style.font.menuFamily; font.pixelSize: Style.font.caption }
                        ScrollView {
                            visible: !root.scratchVisible
                            Layout.fillWidth: true
                            Layout.fillHeight: true
                            clip: true
                            ScrollBar.vertical: ScrollBar {
                                contentItem: Rectangle { implicitWidth: Style.space(6); radius: Style.cornerRadius; color: Color.menu.text; opacity: 0.5 }
                                background: Rectangle { color: "transparent" }
                            }
                            TextArea { text: root.preview; readOnly: true; textFormat: TextEdit.PlainText; wrapMode: TextEdit.WrapAnywhere; color: Color.menu.text; selectionColor: Style.selectionFill; selectedTextColor: Color.menu.selectedText; font.family: Style.font.menuFamily; font.pixelSize: Style.font.body; background: Rectangle { color: Style.normalFill } }
                        }
                        TextField {
                            id: clipboardInput
                            visible: root.scratchVisible
                            Layout.fillWidth: true
                            maximumLength: 16384
                            placeholderText: "Explicit {clipboard} scratch value (not system clipboard)"
                            color: Color.menu.text
                            placeholderTextColor: Color.muted
                            selectionColor: Style.selectionFill
                            selectedTextColor: Color.menu.selectedText
                            font.family: Style.font.menuFamily
                            background: Rectangle { color: Style.normalFill; border.color: Color.menu.border; border.width: 1 }
                            onTextChanged: root.resetScratch()
                        }
                        SbButton {
                            visible: root.scratchVisible
                            text: root.expansionEnabled ? "Scratch expansion: ON" : "Enable scratch expansion"
                            checked: root.expansionEnabled
                            onClicked: { root.expansionEnabled = !root.expansionEnabled; scratch.forceActiveFocus(); root.resetScratch() }
                        }
                        ScrollView {
                            visible: root.scratchVisible
                            Layout.fillWidth: true
                            Layout.fillHeight: true
                            clip: true
                            ScrollBar.vertical: ScrollBar {
                                contentItem: Rectangle { implicitWidth: Style.space(6); radius: Style.cornerRadius; color: Color.menu.text; opacity: 0.5 }
                                background: Rectangle { color: "transparent" }
                            }
                            TextArea {
                                id: scratch
                                placeholderText: "Opt in above, then type !email, !unicode, !multi, or !vars."
                                textFormat: TextEdit.PlainText
                                wrapMode: TextEdit.WrapAnywhere
                                color: Color.menu.text
                                placeholderTextColor: Color.muted
                                selectionColor: Style.selectionFill
                                selectedTextColor: Color.menu.selectedText
                                font.family: Style.font.menuFamily
                                font.pixelSize: Style.font.body
                                background: Rectangle { color: Style.normalFill; border.color: scratch.activeFocus ? Style.focusBorderColor : Color.menu.border; border.width: 1 }
                                onActiveFocusChanged: root.resetScratch()
                                onCursorPositionChanged: root.scratchRevision++
                                onSelectionStartChanged: root.scratchRevision++
                                onSelectionEndChanged: root.scratchRevision++
                                onTextChanged: { root.scratchRevision++; if (!root.injecting) Qt.callLater(root.inspectScratch) }
                                Keys.priority: Keys.BeforeItem
                                Keys.onPressed: function(event) {
                                    root.typedScratch = event.text.length > 0 && event.text.length <= 2 && !(event.modifiers & (Qt.ControlModifier | Qt.MetaModifier | Qt.AltModifier)) && !selectedText.length
                                    if (event.key === Qt.Key_Left || event.key === Qt.Key_Right || event.key === Qt.Key_Up || event.key === Qt.Key_Down || event.key === Qt.Key_Home || event.key === Qt.Key_End) root.resetScratch()
                                }
                            }
                        }
                        SbButton { visible: root.scratchVisible; text: "Insert selected into scratch"; onClicked: root.insertScratch() }
                    }
                }

                Text {
                    Layout.fillWidth: true
                    text: root.status
                    textFormat: Text.PlainText
                    color: Color.menu.text
                    font.family: Style.font.menuFamily
                    font.pixelSize: Style.font.caption
                    wrapMode: Text.Wrap
                    maximumLineCount: 3
                }
            }
        }
    }
}
