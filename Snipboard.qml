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
import qs.Ui

Item {
    id: root
    property string omarchyPath: Quickshell.env("OMARCHY_PATH")
    property var manifest
    property bool opened: false
    property bool ready: false
    property string status: "Starting isolated prototype helper…"
    property string kind: "all"
    property int sequence: 0
    property int wantedSearch: 0
    property int runningSearch: 0
    property int previewSequence: 0
    property string previewRecord: ""
    property var rows: []
    property int selected: 0
    property string preview: ""
    property bool editorVisible: false
    property int pendingEditor: 0
    property int pendingMutation: 0
    property int pendingReload: 0
    property bool scratchVisible: false
    property bool expansionEnabled: false
    property bool injecting: false
    property int scratchRevision: 0
    property int scratchFloor: 0
    property bool typedScratch: false
    property var pendingScratch: null
    property var pendingRender: null
    property int pendingPaste: 0
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
        pasteHandoff.cancel()
        if (editorVisible) { opened = true; return } // A repeated summon must not erase a draft.
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
        if (editorVisible) { snippetEditor.dismiss(true); return }
        opened = false
        pendingEditor = 0
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

    function editSnippet(record) {
        if (!ready || editorVisible || pendingMutation || pendingReload) return
        pendingEditor = send("snippet-get", {record: record})
        expansionEnabled = false
        resetScratch()
        pendingRender = null
    }

    function mutateSnippet(operation, fields) {
        if (!ready || snippetEditor.busy) return
        snippetEditor.busy = true
        snippetEditor.error = ""
        pendingMutation = send(operation, {record: snippetEditor.original.id, revision: snippetEditor.revision, fields: fields})
    }

    function paste() {
        if (!ready || !opened || !activeRow || editorVisible || pendingPaste) return
        pasteHandoff.start(activeRow.id, clipboardInput.text)
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
            status = message.count + " records · synthetic clipboard history · Enter to paste"
            requestSearch()
            return
        }
        if (message.error) {
            status = message.error
            if (message.seq === pendingPaste) {
                pendingPaste = 0
                opened = true
                Qt.callLater(function() { search.forceActiveFocus() })
            }
            if (message.seq === pendingMutation) {
                pendingMutation = 0
                snippetEditor.busy = false
                snippetEditor.confirmDelete = false
                snippetEditor.error = message.error
                if (message.readonly !== undefined) snippetEditor.readOnly = message.readonly
            }
            if (message.seq === pendingEditor) pendingEditor = 0
            if (message.seq === pendingReload) pendingReload = 0
            if (message.op === "search") {
                runningSearch = 0
                if (message.seq !== wantedSearch) startSearch()
            }
            return
        }
        if (message.op === "snippet-get") {
            if (message.seq !== pendingEditor || !opened) return
            pendingEditor = 0
            editorVisible = true
            snippetEditor.load(message.snippet, message.revision, message.readonly)
            return
        }
        if (message.op === "snippet-save" || message.op === "snippet-delete") {
            if (message.seq !== pendingMutation) return
            pendingMutation = 0
            snippetEditor.busy = false
            editorVisible = false
            previewRecord = ""
            preview = ""
            previewSequence = 0
            resetScratch()
            requestSearch()
            search.forceActiveFocus()
            status = message.deleted ? "Snippet deleted from prototype library." : "Snippet saved to prototype library."
            return
        }
        if (message.op === "library-reload") {
            if (message.seq !== pendingReload) return
            pendingReload = 0
            previewRecord = ""
            preview = ""
            previewSequence = 0
            requestSearch()
            status = "Library reloaded from disk. No clipboard history accessed."
            return
        }
        if (message.op === "search") {
            runningSearch = 0
            if (message.seq === wantedSearch) {
                rows = message.rows
                // Keep delegates and unchanged roles across query updates.
                for (let i = 0; i < rows.length; i++) {
                    const row = rows[i]
                    const value = {recordId: row.id, name: row.name, kind: row.kind, summary: row.summary}
                    if (i >= displayModel.count) displayModel.append(value)
                    else {
                        const old = displayModel.get(i)
                        if (old.recordId !== row.id || old.name !== row.name || old.summary !== row.summary || old.kind !== row.kind) displayModel.set(i, value)
                    }
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
        } else if (message.op === "paste" && message.seq === pendingPaste) {
            pendingPaste = 0
            status = "Paste shortcut sent."
            send("used", {record: message.record})
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
        command: ["setpriv", "--pdeathsig", "TERM", root.sourceDirectory + "/build/release/snipboard-worker"]
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
            pasteHandoff.cancel()
            root.pendingPaste = 0
            root.ready = false
            root.runningSearch = 0
            root.pendingEditor = 0
            root.pendingMutation = 0
            root.pendingReload = 0
            snippetEditor.busy = false
            snippetEditor.readOnly = true
            root.status = "Helper stopped. Invalid storage is preserved; rebuild/reload after resolving errors."
        }
    }

    SbPasteHandoff {
        id: pasteHandoff
        onHideRequested: root.close()
        onPasteRequested: function(record, clipboard) {
            root.pendingPaste = root.send("paste", {record: record, clipboard: clipboard})
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

    component Caption: Text {
        color: Color.muted
        font.family: Style.font.menuFamily
        font.pixelSize: Style.font.caption
        textFormat: Text.PlainText
        elide: Text.ElideRight
    }

    component Divider: Rectangle {
        implicitWidth: Style.normalBorderWidth
        implicitHeight: Style.normalBorderWidth
        color: Util.alpha(Color.menu.border, 0.28)
    }

    component Scrollbar: ScrollBar {
        id: bar
        contentItem: Rectangle { implicitWidth: Style.space(4); radius: width / 2; color: Color.menu.text; opacity: bar.active ? 0.5 : 0.2 }
        background: Item {}
    }

    component Input: TextField {
        color: Color.menu.text
        placeholderTextColor: Color.muted
        selectionColor: Style.selectionFill
        selectedTextColor: Color.menu.selectedText
        font.family: Style.font.menuFamily
        font.pixelSize: Style.font.body
        leftPadding: Style.space(10)
        rightPadding: Style.space(10)
        topPadding: Style.space(7)
        bottomPadding: Style.space(7)
        background: Rectangle { color: Style.normalFill; border.color: parent.activeFocus ? Color.accent : Color.menu.border; border.width: 1; radius: Style.cornerRadius }
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

        BorderSurface {
            id: card
            anchors.centerIn: parent
            width: Math.min(Style.space(900), panel.width - Style.gapsOut * 2)
            height: Math.min(Style.space(600), panel.height - Style.gapsOut * 2)
            radius: Style.cornerRadius
            color: Color.menu.background
            borderSpec: Border.surfaceSpec("menu", "border", Color.menu.border, Math.max(1, Style.space(2)))
            padding: Style.spacing.panelPadding
            MouseArea { anchors.fill: parent }

            ColumnLayout {
                anchors.fill: parent
                anchors.topMargin: card.contentTopInset
                anchors.rightMargin: card.contentRightInset
                anchors.bottomMargin: card.contentBottomInset
                anchors.leftMargin: card.contentLeftInset
                spacing: Style.space(10)

                // Header: identity, mode toggle, close.
                RowLayout {
                    Layout.fillWidth: true
                    spacing: Style.space(8)
                    Text { text: "󰅍"; color: Color.accent; font.family: Style.font.menuFamily; font.pixelSize: Style.font.iconLarge }
                    Text { text: "Snipboard"; color: Color.menu.text; font.family: Style.font.menuFamily; font.pixelSize: Style.font.title; font.bold: true }
                    Rectangle {
                        implicitWidth: badge.implicitWidth + Style.space(12)
                        implicitHeight: badge.implicitHeight + Style.space(4)
                        radius: height / 2
                        color: "transparent"
                        border.color: Util.alpha(Color.accent, 0.6)
                        border.width: 1
                        Caption { id: badge; anchors.centerIn: parent; text: "prototype"; color: Color.accent }
                    }
                    Item { Layout.fillWidth: true }
                    SbButton {
                        visible: !root.editorVisible
                        text: "Scratch"
                        glyph: "󰏫"
                        checkable: true
                        checked: root.scratchVisible
                        onClicked: { root.scratchVisible = !root.scratchVisible; root.resetScratch() }
                    }
                    SbButton { glyph: "󰅖"; hint: "Close (Esc)"; onClicked: root.close() }
                }

                // Search: large, quiet, accent underline when focused.
                Item {
                    visible: !root.editorVisible
                    Layout.fillWidth: true
                    implicitHeight: search.implicitHeight
                    Text {
                        id: searchGlyph
                        anchors.left: parent.left
                        anchors.verticalCenter: parent.verticalCenter
                        text: "󰍉"
                        color: search.activeFocus ? Color.accent : Color.muted
                        font.family: Style.font.menuFamily
                        font.pixelSize: Style.font.heading
                    }
                    TextField {
                        id: search
                        anchors.left: searchGlyph.right
                        anchors.leftMargin: Style.space(10)
                        anchors.right: parent.right
                        placeholderText: "Search clipboard and snippets…"
                        maximumLength: 256
                        color: Color.menu.text
                        placeholderTextColor: Color.muted
                        selectionColor: Style.selectionFill
                        selectedTextColor: Color.menu.selectedText
                        font.family: Style.font.menuFamily
                        font.pixelSize: Style.font.heading
                        leftPadding: 0
                        topPadding: Style.space(8)
                        bottomPadding: Style.space(8)
                        background: Item {}
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
                    Rectangle {
                        anchors.left: parent.left
                        anchors.right: parent.right
                        anchors.bottom: parent.bottom
                        height: search.activeFocus ? Math.max(1, Style.space(2)) : Style.normalBorderWidth
                        color: search.activeFocus ? Color.accent : Util.alpha(Color.menu.border, 0.28)
                    }
                }

                // Filters and actions.
                RowLayout {
                    visible: !root.editorVisible
                    Layout.fillWidth: true
                    spacing: Style.space(6)
                    Repeater {
                        model: [{label: "All", value: "all", glyph: "󰒺"}, {label: "Clipboard", value: "clipboard", glyph: "󰅌"}, {label: "Snippets", value: "snippet", glyph: "󰅩"}]
                        SbButton {
                            required property var modelData
                            text: modelData.label
                            glyph: modelData.glyph
                            checked: root.kind === modelData.value
                            onClicked: { root.kind = modelData.value; root.requestSearch(); search.forceActiveFocus() }
                        }
                    }
                    Item { Layout.fillWidth: true }
                    SbButton { text: "New"; glyph: "󰐕"; enabled: root.ready && !root.pendingEditor && !root.pendingReload; onClicked: root.editSnippet("") }
                    SbButton { text: "Edit"; glyph: "󰏫"; enabled: root.ready && !root.pendingEditor && !root.pendingReload && root.activeRow !== null && root.activeRow.kind === "snippet"; onClicked: root.editSnippet(root.activeRow.id) }
                    SbButton { text: "Copy"; glyph: "󰆏"; enabled: root.activeRow !== null; onClicked: root.copyOnly() }
                    SbButton { glyph: "󰑐"; hint: "Reload library from disk"; enabled: root.ready && !root.pendingEditor && !root.pendingReload; onClicked: { root.expansionEnabled = false; root.resetScratch(); root.pendingRender = null; root.pendingReload = root.send("library-reload", {}) } }
                }

                SbSnippetEditor {
                    id: snippetEditor
                    visible: root.editorVisible
                    Layout.fillWidth: true
                    Layout.fillHeight: true
                    onSaveRequested: function(fields) { root.mutateSnippet("snippet-save", fields) }
                    onDeleteRequested: root.mutateSnippet("snippet-delete", null)
                    onDismissed: function(closePanel) {
                        root.editorVisible = false
                        if (closePanel) root.close()
                        else search.forceActiveFocus()
                    }
                }

                RowLayout {
                    visible: !root.editorVisible
                    Layout.fillWidth: true
                    Layout.fillHeight: true
                    spacing: Style.spacing.panelPadding
                    ListView {
                        id: results
                        Layout.fillWidth: true
                        Layout.fillHeight: true
                        Layout.preferredWidth: card.width / 2
                        clip: true
                        model: displayModel
                        boundsBehavior: Flickable.StopAtBounds
                        spacing: Style.space(2)
                        ScrollBar.vertical: Scrollbar {}
                        delegate: Rectangle {
                            id: row
                            required property string recordId
                            required property string name
                            required property string kind
                            required property string summary
                            required property int index
                            readonly property bool current: index === root.selected
                            width: results.width
                            height: Math.max(Style.space(50), Style.font.body + Style.font.caption + Style.space(26))
                            radius: Style.cornerRadius
                            color: current ? Color.menu.selectedBackground : "transparent"
                            border.color: current ? Color.menu.selectedBorder : "transparent"
                            border.width: current ? Style.selectedBorderWidth : 0
                            RowLayout {
                                anchors.fill: parent
                                anchors.leftMargin: Style.space(10)
                                anchors.rightMargin: Style.space(12)
                                spacing: Style.space(10)
                                Text {
                                    Layout.preferredWidth: Style.font.iconLarge + Style.space(4)
                                    horizontalAlignment: Text.AlignHCenter
                                    text: row.kind === "snippet" ? "󰅩" : "󰅌"
                                    color: row.current ? Color.menu.selectedText : Color.muted
                                    font.family: Style.font.menuFamily
                                    font.pixelSize: Style.font.iconLarge
                                }
                                ColumnLayout {
                                    Layout.fillWidth: true
                                    spacing: Style.space(2)
                                    Text { Layout.fillWidth: true; text: row.name; textFormat: Text.PlainText; color: row.current ? Color.menu.selectedText : Color.menu.text; font.family: Style.font.menuFamily; font.pixelSize: Style.font.title; elide: Text.ElideRight }
                                    Text { Layout.fillWidth: true; text: row.summary; textFormat: Text.PlainText; color: Color.menu.text; opacity: 0.55; font.family: Style.font.menuFamily; font.pixelSize: Style.font.caption; elide: Text.ElideRight }
                                }
                            }
                            MouseArea {
                                anchors.fill: parent
                                hoverEnabled: true
                                cursorShape: Qt.PointingHandCursor
                                onPositionChanged: root.selected = row.index
                                onClicked: { root.selected = row.index; root.paste() }
                            }
                        }
                        Column {
                            anchors.centerIn: parent
                            width: parent.width
                            spacing: Style.space(8)
                            visible: !root.rows.length
                            Text {
                                width: parent.width
                                horizontalAlignment: Text.AlignHCenter
                                text: root.ready ? "󰅌" : "󰔟"
                                color: Color.menu.selectedText
                                opacity: 0.8
                                font.family: Style.font.menuFamily
                                font.pixelSize: Style.font.displayLarge
                            }
                            Text {
                                width: parent.width
                                horizontalAlignment: Text.AlignHCenter
                                text: !root.ready ? "Loading…" : search.text.length ? "No matches for “" + search.text + "”" : "Nothing here yet"
                                textFormat: Text.PlainText
                                elide: Text.ElideMiddle
                                color: Color.menu.text
                                opacity: 0.7
                                font.family: Style.font.menuFamily
                                font.pixelSize: Style.font.title
                            }
                        }
                    }

                    Divider { Layout.fillHeight: true }

                    ColumnLayout {
                        Layout.fillWidth: true
                        Layout.fillHeight: true
                        Layout.preferredWidth: card.width / 2
                        spacing: Style.space(8)
                        RowLayout {
                            Layout.fillWidth: true
                            Caption { text: root.scratchVisible ? "SCRATCHPAD" : "PREVIEW"; font.letterSpacing: 1; font.bold: true }
                            Caption {
                                Layout.fillWidth: true
                                horizontalAlignment: Text.AlignRight
                                text: root.scratchVisible ? "Disposable input · never an external app" : root.activeRow ? (root.activeRow.kind === "snippet" ? "Snippet · exact text" : "Clipboard · exact text") : ""
                            }
                        }
                        ScrollView {
                            visible: !root.scratchVisible
                            Layout.fillWidth: true
                            Layout.fillHeight: true
                            clip: true
                            ScrollBar.vertical: Scrollbar {}
                            TextArea { text: root.preview; readOnly: true; textFormat: TextEdit.PlainText; wrapMode: TextEdit.WrapAtWordBoundaryOrAnywhere; color: Color.menu.text; selectionColor: Style.selectionFill; selectedTextColor: Color.menu.selectedText; font.family: Style.font.menuFamily; font.pixelSize: Style.font.title; leftPadding: 0; rightPadding: Style.space(8); topPadding: 0; background: Item {} }
                        }
                        Input {
                            id: clipboardInput
                            visible: root.scratchVisible
                            Layout.fillWidth: true
                            maximumLength: 16384
                            placeholderText: "{clipboard} value (not the system clipboard)"
                            onTextChanged: root.resetScratch()
                        }
                        SbButton {
                            visible: root.scratchVisible
                            Layout.fillWidth: true
                            text: root.expansionEnabled ? "Scratch expansion on" : "Enable scratch expansion"
                            glyph: root.expansionEnabled ? "󰄬" : "󰐊"
                            checked: root.expansionEnabled
                            onClicked: { root.expansionEnabled = !root.expansionEnabled; scratch.forceActiveFocus(); root.resetScratch() }
                        }
                        ScrollView {
                            visible: root.scratchVisible
                            Layout.fillWidth: true
                            Layout.fillHeight: true
                            clip: true
                            ScrollBar.vertical: Scrollbar {}
                            TextArea {
                                id: scratch
                                placeholderText: "Enable expansion, then type !email, !unicode, !multi, or !vars."
                                textFormat: TextEdit.PlainText
                                wrapMode: TextEdit.WrapAnywhere
                                color: Color.menu.text
                                placeholderTextColor: Color.muted
                                selectionColor: Style.selectionFill
                                selectedTextColor: Color.menu.selectedText
                                font.family: Style.font.menuFamily
                                font.pixelSize: Style.font.body
                                padding: Style.space(10)
                                background: Rectangle { color: Style.normalFill; border.color: scratch.activeFocus ? Color.accent : Color.menu.border; border.width: 1; radius: Style.cornerRadius }
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
                        SbButton { visible: root.scratchVisible; Layout.fillWidth: true; text: "Insert selected into scratch"; glyph: "󰁅"; enabled: root.activeRow !== null; onClicked: root.insertScratch() }
                    }
                }

                Divider { Layout.fillWidth: true }

                // Footer: status on the left, key hints on the right.
                RowLayout {
                    Layout.fillWidth: true
                    spacing: Style.space(12)
                    Caption {
                        Layout.fillWidth: true
                        font.pixelSize: Style.font.bodySmall
                        text: root.status
                        wrapMode: Text.Wrap
                        maximumLineCount: 2
                    }
                    Caption {
                        visible: !root.editorVisible
                        text: "↑↓ select   ⏎ paste   ^C copy   esc close"
                        font.pixelSize: Style.font.bodySmall
                        elide: Text.ElideNone
                    }
                }
            }
        }
    }
}
