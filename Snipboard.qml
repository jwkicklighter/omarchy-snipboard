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
import qs.Commons as Commons
import qs.Ui
import "SbActions.js" as SbActions

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
    property string previewKeyword: ""
    property real previewCreated: 0
    property string shownQuery: ""
    property string runningQuery: ""
    // Right arrow / Ctrl+K: actions for the row that was selected when opened.
    property bool actionsOpen: false
    property var actionTarget: null
    property string actionQuery: ""
    property int actionSelected: 0
    readonly property var actionRows: SbActions.filter(SbActions.forRow(actionTarget), actionQuery)
    property bool editorVisible: false
    property int pendingEditor: 0
    property int pendingMutation: 0
    property int pendingReload: 0
    property int pendingHistoryDelete: 0
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
    // The preview follows the action target while the action menu is open.
    readonly property var previewSubject: actionsOpen ? actionTarget : activeRow
    // Secondary text dims the menu foreground, as Omarchy's own menu does.
    // The theme's `muted` role is often too dark to read on the card.
    readonly property color textColor: Commons.Color.menu.text
    readonly property color dimText: Util.alpha(Commons.Color.menu.text, 0.55)
    readonly property color faintText: Util.alpha(Commons.Color.menu.text, 0.38)
    readonly property color hairline: Util.alpha(Commons.Color.menu.text, 0.12)
    readonly property color keyFill: Util.alpha(Commons.Color.menu.text, 0.08)

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
        actionsOpen = false
        expansionEnabled = false
        scratchVisible = false
        resetScratch()
        shownQuery = "" // Every summon starts at the top of the list.
        search.text = ""
        kind = "all"
        requestSearch()
        Qt.callLater(function() { search.forceActiveFocus() })
    }

    function close() {
        if (editorVisible) { snippetEditor.dismiss(true); return }
        opened = false
        actionsOpen = false
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
        runningQuery = kind + "\n" + search.text
        helper.write(JSON.stringify({op: "search", seq: runningSearch, query: search.text, kind: kind}) + "\n")
    }

    function select(delta) {
        if (!rows.length) return
        selected = (selected + delta + rows.length) % rows.length
        results.positionViewAtIndex(selected, ListView.Contain)
    }

    function loadPreview() {
        if (previewSubject && previewSubject.id === previewRecord) return
        previewTimer.restart()
    }

    function copyOnly(row) {
        row = row || activeRow
        if (row) send("copy", {record: row.id})
    }

    function deleteHistory(row) {
        row = row || activeRow
        if (!ready || !opened || editorVisible || pendingHistoryDelete || pendingPaste || !row || row.kind !== "clipboard") return
        pendingHistoryDelete = send("clipboard-delete", {record: row.id})
    }

    function reloadLibrary() {
        if (!ready || pendingEditor || pendingReload) return
        expansionEnabled = false
        resetScratch()
        pendingRender = null
        pendingReload = send("library-reload", {})
    }

    function cycleKind(delta) {
        const kinds = ["all", "clipboard", "snippet"]
        kind = kinds[(kinds.indexOf(kind) + delta + kinds.length) % kinds.length]
        requestSearch()
    }

    // Ctrl+1–9 pastes by position within the visible rows, like Alfred.
    function quickPaste(position) {
        const index = results.firstVisible + position - 1
        if (index < 0 || index >= rows.length) return
        selected = index
        paste()
    }

    function openActions() {
        if (!ready || !opened || editorVisible || actionsOpen) return
        actionTarget = activeRow
        actionQuery = ""
        actionSearch.text = ""
        actionSelected = 0
        actionsOpen = true
        actionSearch.forceActiveFocus()
    }

    function closeActions() {
        if (!actionsOpen) return
        actionsOpen = false
        actionTarget = null
        actionSearch.text = ""
        loadPreview()
        if (opened && !editorVisible) search.forceActiveFocus()
    }

    function selectAction(delta) {
        if (!actionRows.length) return
        actionSelected = (actionSelected + delta + actionRows.length) % actionRows.length
        actionList.positionViewAtIndex(actionSelected, ListView.Contain)
    }

    function runAction(action) {
        if (!action) return
        const target = actionTarget
        closeActions()
        if (action.id === "paste") paste(target)
        else if (action.id === "copy") copyOnly(target)
        else if (action.id === "edit" && target) editSnippet(target.id)
        else if (action.id === "duplicate" && target) editSnippet(target.id, true)
        else if (action.id === "scratch") { scratchVisible = true; insertScratch(target) }
        else if (action.id === "delete") deleteHistory(target)
        else if (action.id === "new") editSnippet("")
        else if (action.id === "toggle-scratch") { scratchVisible = !scratchVisible; resetScratch() }
        else if (action.id === "reload") reloadLibrary()
    }

    function rowGlyph(kind, text) {
        if (kind === "snippet") return "󰅩"
        if (/^https?:\/\//.test(text)) return "󰌷"
        if (/^(\/|~\/)\S*$/.test(text)) return "󰈔"
        return "󰅌"
    }

    function editSnippet(record, draft) {
        if (!ready || editorVisible || pendingEditor || pendingMutation || pendingReload) return
        pendingEditor = send(draft ? "snippet-draft" : "snippet-get", {record: record})
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

    function paste(row) {
        row = row || activeRow
        if (!ready || !opened || !row || editorVisible || pendingPaste) return
        pasteHandoff.start(row.id)
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

    function insertScratch(row) {
        row = row || activeRow
        if (!row) return
        resetScratch()
        const seq = send("render", {record: row.id, clipboard: clipboardInput.text})
        pendingRender = {seq: seq, record: row.id, revision: scratchRevision}
    }

    function receive(message) {
        if (message.op === "ready") {
            ready = true
            status = message.count + " records · recording clipboard text"
            clipboardWatcher.running = true
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
            if (message.seq === pendingHistoryDelete) pendingHistoryDelete = 0
            if (message.op === "search") {
                runningSearch = 0
                if (message.seq !== wantedSearch) startSearch()
            }
            return
        }
        if (message.op === "snippet-get" || message.op === "snippet-draft") {
            if (message.seq !== pendingEditor || !opened) return
            pendingEditor = 0
            editorVisible = true
            snippetEditor.load(message.snippet, message.revision, message.readonly, message.op === "snippet-draft")
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
            status = "Library reloaded from disk."
            return
        }
        if (message.op === "clipboard-delete") {
            if (message.seq !== pendingHistoryDelete) return
            pendingHistoryDelete = 0
            previewTimer.stop()
            previewRecord = ""
            preview = ""
            previewSequence = 0
            pendingRender = null
            resetScratch()
            requestSearch()
            if (opened && !editorVisible) search.forceActiveFocus()
            status = "Deleted from history. System clipboard unchanged."
            return
        }
        if (message.op === "clipboard-capture") {
            if (opened && !editorVisible) requestSearch()
            return
        }
        if (message.op === "search") {
            runningSearch = 0
            if (message.seq === wantedSearch) {
                // A clipboard capture re-runs the same query; keep the cursor on its row.
                const keepId = shownQuery === runningQuery && activeRow ? activeRow.id : ""
                shownQuery = runningQuery
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
                let keep = 0
                for (let i = 0; keepId && i < rows.length; i++) if (rows[i].id === keepId) { keep = i; break }
                selected = keep
                results.positionViewAtIndex(selected, ListView.Contain)
                matchMs = message.matchMs
                assignedSequence = message.seq
                pendingFrameSequence = message.seq
                pendingFrameInputAt = inputAt
                // Model assignment timing only, NOT presented/visible frame latency.
                modelLatency = modelLatency.concat([Date.now() - inputAt]).slice(-1000)
                loadPreview()
                if (openedAt && opened) openLatency = openLatency.concat([Date.now() - openedAt]).slice(-1000)
            } else startSearch() // Coalesce rapid input: at most one stale query runs.
        } else if (message.op === "preview" && message.seq === previewSequence && previewSubject && previewSubject.id === previewRecord) {
            preview = message.body
            previewKeyword = message.keyword || ""
            previewCreated = message.created || 0
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
                                 clipboardCaptureVersion: 1, clipboardWatcherRunning: clipboardWatcher.running,
                                  historyDeleteVersion: 1, historyDeletePending: pendingHistoryDelete !== 0,
                                  overlayWindowVersion: 1, windowVisible: panel.visible,
                                  windowWidth: panel.width, windowHeight: panel.height,
                                  surfaceColor: panel.color.toString(), cardWidth: card.width, cardHeight: card.height,
                                  uiVersion: 2, actionsOpen: actionsOpen, actionCount: actionRows.length,
                                  colors: {accent: Commons.Color.accent.toString(), text: textColor.toString(), dim: dimText.toString(),
                                           card: Commons.Color.menu.background.toString(), selected: Commons.Color.menu.selectedBackground.toString()},
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
    onActionQueryChanged: actionSelected = 0
    onExpansionEnabledChanged: resetScratch()
    Keys.onEscapePressed: function(event) { root.close(); event.accepted = true }
    Component.onCompleted: helper.running = true
    Component.onDestruction: {
        expansionEnabled = false
        clipboardWatcher.running = false
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
        id: clipboardWatcher
        // Capture the regular Wayland clipboard, never the primary selection.
        // Each callback frames its stdin as JSON; no text is passed as arguments.
        command: ["setpriv", "--pdeathsig", "TERM", "wl-paste", "--type", "text", "--watch",
                  "setpriv", "--pdeathsig", "TERM", root.sourceDirectory + "/build/release/snipboard-worker", "--clipboard-event"]
        stdout: SplitParser {
            onRead: function(line) {
                try {
                    const event = JSON.parse(line)
                    if (root.ready && typeof event.text === "string") root.send("clipboard-capture", {text: event.text})
                } catch (error) { root.status = "Clipboard event rejected; private details suppressed." }
            }
        }
        stderr: SplitParser { onRead: function(line) { root.status = "Clipboard capture unavailable. Check wl-paste and Wayland data-control support." } }
        onExited: if (root.ready) root.status = "Clipboard capture stopped. Reload the plugin to retry."
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
            clipboardWatcher.running = false
            pasteHandoff.cancel()
            root.pendingPaste = 0
            root.ready = false
            root.runningSearch = 0
            root.pendingEditor = 0
            root.pendingMutation = 0
            root.pendingReload = 0
            root.pendingHistoryDelete = 0
            snippetEditor.busy = false
            snippetEditor.readOnly = true
            root.status = "Helper stopped. Invalid storage is preserved; rebuild/reload after resolving errors."
        }
    }

    SbPasteHandoff {
        id: pasteHandoff
        onHideRequested: root.close()
        onPasteRequested: function(record) {
            root.pendingPaste = root.send("paste", {record: record})
        }
    }

    ListModel { id: displayModel }
    Timer {
        id: previewTimer
        interval: 120
        onTriggered: {
            if (!root.previewSubject) { root.preview = ""; root.previewRecord = ""; return }
            root.previewRecord = root.previewSubject.id
            root.previewSequence = root.send("preview", {record: root.previewRecord})
        }
    }

    component Caption: Text {
        color: Util.alpha(Commons.Color.menu.text, 0.55)
        font.family: Style.font.menuFamily
        font.pixelSize: Style.font.caption
        textFormat: Text.PlainText
        elide: Text.ElideRight
    }

    component Divider: Rectangle {
        implicitWidth: Style.spacing.hairline
        implicitHeight: Style.spacing.hairline
        color: Util.alpha(Commons.Color.menu.text, 0.12)
    }

    component Scrollbar: ScrollBar {
        id: bar
        contentItem: Rectangle { implicitWidth: Style.space(4); radius: width / 2; visible: bar.size < 1; color: Commons.Color.menu.text; opacity: bar.active ? 0.5 : 0.2 }
        background: Item {}
    }

    // Raycast-style key cap: "Ctrl" "K".
    component KeyCap: Rectangle {
        property alias text: capLabel.text
        property bool lit: false
        implicitWidth: Math.max(implicitHeight, capLabel.implicitWidth + Style.space(10))
        implicitHeight: capLabel.implicitHeight + Style.space(6)
        radius: Math.min(Style.cornerRadius, Style.space(4))
        color: lit ? Util.alpha(Commons.Color.menu.selectedText, 0.16) : Util.alpha(Commons.Color.menu.text, 0.08)
        Text {
            id: capLabel
            anchors.centerIn: parent
            color: parent.lit ? Commons.Color.menu.selectedText : Util.alpha(Commons.Color.menu.text, 0.55)
            font.family: Style.font.menuFamily
            font.pixelSize: Style.font.caption
            textFormat: Text.PlainText
        }
    }

    // "Paste ⏎" in the footer.
    component Hint: Row {
        property alias label: hintLabel.text
        property var keys: []
        spacing: Style.space(6)
        Text {
            id: hintLabel
            anchors.verticalCenter: parent.verticalCenter
            color: Commons.Color.menu.text
            font.family: Style.font.menuFamily
            font.pixelSize: Style.font.bodySmall
            textFormat: Text.PlainText
        }
        Repeater {
            model: parent.keys
            KeyCap { required property string modelData; anchors.verticalCenter: parent.verticalCenter; text: modelData }
        }
    }

    component Input: TextField {
        color: Commons.Color.menu.text
        placeholderTextColor: Util.alpha(Commons.Color.menu.text, 0.38)
        selectionColor: Style.selectionFill
        selectedTextColor: Commons.Color.menu.selectedText
        font.family: Style.font.menuFamily
        font.pixelSize: Style.font.body
        leftPadding: Style.space(10)
        rightPadding: Style.space(10)
        topPadding: Style.space(7)
        bottomPadding: Style.space(7)
        background: Rectangle { color: Style.normalFill; border.color: parent.activeFocus ? Commons.Color.accent : Util.alpha(Commons.Color.menu.text, 0.12); border.width: 1; radius: Style.cornerRadius }
    }

    component SearchField: TextField {
        color: Commons.Color.menu.text
        placeholderTextColor: Util.alpha(Commons.Color.menu.text, 0.38)
        selectionColor: Style.selectionFill
        selectedTextColor: Commons.Color.menu.selectedText
        font.family: Style.font.menuFamily
        font.pixelSize: Math.round(Style.font.heading * 1.25)
        leftPadding: 0
        rightPadding: 0
        topPadding: 0
        bottomPadding: 0
        background: Item {}
        Keys.priority: Keys.BeforeItem
    }

    // A list row in the Omarchy menu treatment: selected rows take the menu
    // selection fill/border and the selected-text color.
    component ListRow: BorderSurface {
        id: listRow
        property bool current: false
        property string glyph: ""
        property string title: ""
        property string subtitle: ""
        property bool danger: false
        default property alias trailing: trailingRow.data
        signal activated()
        signal secondary()
        signal hovered()
        height: Math.max(Style.space(36), Style.font.title + Style.space(18))
        radius: Style.cornerRadius
        color: current ? Commons.Color.menu.selectedBackground : "transparent"
        borderSpec: current ? Border.surfaceSpec("menu", "selected-border", Commons.Color.menu.selectedBorder, 0) : Border.none()
        readonly property color tone: danger ? Commons.Color.urgent : current ? Commons.Color.menu.selectedText : Commons.Color.menu.text
        RowLayout {
            anchors.fill: parent
            anchors.leftMargin: Style.space(10)
            anchors.rightMargin: Style.space(10)
            spacing: Style.space(10)
            Text {
                Layout.preferredWidth: Style.font.iconLarge + Style.space(2)
                horizontalAlignment: Text.AlignHCenter
                text: listRow.glyph
                color: listRow.danger ? Commons.Color.urgent : listRow.current ? Commons.Color.menu.selectedText : Util.alpha(Commons.Color.menu.text, 0.55)
                font.family: Style.font.menuFamily
                font.pixelSize: Style.font.iconLarge
            }
            Text {
                Layout.fillWidth: !listRow.subtitle.length
                Layout.maximumWidth: listRow.subtitle.length ? implicitWidth : Number.POSITIVE_INFINITY
                text: listRow.title
                textFormat: Text.PlainText
                elide: Text.ElideRight
                maximumLineCount: 1
                color: listRow.tone
                font.family: Style.font.menuFamily
                font.pixelSize: Style.font.title
            }
            Text {
                visible: listRow.subtitle.length > 0
                Layout.fillWidth: true
                text: listRow.subtitle
                textFormat: Text.PlainText
                elide: Text.ElideRight
                maximumLineCount: 1
                color: Util.alpha(Commons.Color.menu.text, 0.38)
                font.family: Style.font.menuFamily
                font.pixelSize: Style.font.bodySmall
            }
            RowLayout { id: trailingRow; spacing: Style.space(4) }
        }
        MouseArea {
            anchors.fill: parent
            hoverEnabled: true
            acceptedButtons: Qt.LeftButton | Qt.RightButton
            cursorShape: Qt.PointingHandCursor
            onPositionChanged: listRow.hovered()
            onClicked: function(mouse) { if (mouse.button === Qt.RightButton) listRow.secondary(); else listRow.activated() }
        }
    }

    // A transparent shell overlay, not a tiled desktop window. Only the card
    // paints a background; never add a full-screen scrim here.
    PanelWindow {
        id: panel
        visible: root.opened
        anchors { top: true; bottom: true; left: true; right: true }
        color: "transparent"
        exclusionMode: ExclusionMode.Ignore
        WlrLayershell.namespace: "omarchy-snipboard"
        WlrLayershell.layer: WlrLayer.Overlay
        WlrLayershell.keyboardFocus: WlrKeyboardFocus.Exclusive

        MouseArea { anchors.fill: parent; onClicked: root.close() }

        BorderSurface {
            id: card
            anchors.centerIn: parent
            width: Math.min(Style.space(900), panel.width - Style.gapsOut * 2)
            height: Math.min(Style.space(600), panel.height - Style.gapsOut * 2)
            radius: Style.cornerRadius
            color: Commons.Color.menu.background
            borderSpec: Border.surfaceSpec("menu", "border", Commons.Color.menu.border, Math.max(1, Style.space(2)))
            clip: true
            Keys.onEscapePressed: function(event) { root.close(); event.accepted = true }
            MouseArea { anchors.fill: parent }

            readonly property int gutter: Style.spacing.huge

            ColumnLayout {
                anchors.fill: parent
                anchors.topMargin: card.contentTopInset
                anchors.rightMargin: card.contentRightInset
                anchors.bottomMargin: card.contentBottomInset
                anchors.leftMargin: card.contentLeftInset
                spacing: 0

                // Header: one large search line. In the action menu it filters
                // actions and shows the target as a breadcrumb.
                Item {
                    visible: !root.editorVisible
                    Layout.fillWidth: true
                    implicitHeight: Math.round(Style.font.heading * 1.25) + Style.space(36)
                    RowLayout {
                        anchors.fill: parent
                        anchors.leftMargin: card.gutter
                        anchors.rightMargin: card.gutter
                        spacing: Style.space(12)
                        Text {
                            visible: root.actionsOpen
                            text: "󰁍"
                            color: Commons.Color.accent
                            font.family: Style.font.menuFamily
                            font.pixelSize: Style.font.display
                            MouseArea {
                                anchors.fill: parent
                                enabled: root.actionsOpen
                                cursorShape: enabled ? Qt.PointingHandCursor : Qt.ArrowCursor
                                onClicked: root.closeActions()
                            }
                        }
                        Rectangle {
                            visible: root.actionsOpen && root.actionTarget !== null
                            Layout.maximumWidth: card.width * 0.32
                            implicitWidth: crumb.implicitWidth + Style.space(16)
                            implicitHeight: crumb.implicitHeight + Style.space(8)
                            radius: Style.cornerRadius
                            color: root.keyFill
                            Text {
                                id: crumb
                                anchors.fill: parent
                                anchors.leftMargin: Style.space(8)
                                anchors.rightMargin: Style.space(8)
                                verticalAlignment: Text.AlignVCenter
                                text: root.actionTarget ? root.actionTarget.name : ""
                                textFormat: Text.PlainText
                                elide: Text.ElideRight
                                color: root.dimText
                                font.family: Style.font.menuFamily
                                font.pixelSize: Style.font.body
                            }
                        }
                        SearchField {
                            id: search
                            visible: !root.actionsOpen
                            Layout.fillWidth: true
                            placeholderText: "Search clipboard and snippets…"
                            maximumLength: 256
                            onTextChanged: root.requestSearch()
                            Keys.onPressed: function(event) {
                                const command = event.modifiers & (Qt.ControlModifier | Qt.MetaModifier)
                                if (event.key === Qt.Key_Escape) {
                                    if (text.length) clear(); else root.close()
                                } else if (event.key === Qt.Key_Down) root.select(1)
                                else if (event.key === Qt.Key_Up) root.select(-1)
                                else if (event.key === Qt.Key_PageDown) root.select(6)
                                else if (event.key === Qt.Key_PageUp) root.select(-6)
                                else if (event.key === Qt.Key_Return || event.key === Qt.Key_Enter) root.paste()
                                else if (event.key === Qt.Key_Tab) root.cycleKind(1)
                                else if (event.key === Qt.Key_Backtab) root.cycleKind(-1)
                                else if (event.key === Qt.Key_Right && !command && cursorPosition === text.length && !selectedText.length) root.openActions()
                                else if (event.key === Qt.Key_K && command) root.openActions()
                                else if (command && event.key >= Qt.Key_1 && event.key <= Qt.Key_9) root.quickPaste(event.key - Qt.Key_0)
                                else if (event.key === Qt.Key_N && command) root.editSnippet("")
                                else if (event.key === Qt.Key_E && command) {
                                    if (root.activeRow && root.activeRow.kind === "snippet") root.editSnippet(root.activeRow.id)
                                } else if (event.key === Qt.Key_S && command) {
                                    if (root.activeRow && root.activeRow.kind === "clipboard") root.editSnippet(root.activeRow.id, true)
                                } else if (event.key === Qt.Key_D && command) {
                                    if (root.activeRow && root.activeRow.kind === "snippet") root.editSnippet(root.activeRow.id, true)
                                } else if (event.key === Qt.Key_Delete && command) {
                                    root.deleteHistory()
                                } else if (event.key === Qt.Key_C && command) {
                                    if (selectedText.length) { copy(); event.accepted = true; return }
                                    root.copyOnly()
                                } else return // Preserve editing, selection, Home/End, IME, and paste.
                                event.accepted = true
                            }
                        }
                        SearchField {
                            id: actionSearch
                            visible: root.actionsOpen
                            Layout.fillWidth: true
                            placeholderText: "Search actions…"
                            maximumLength: 128
                            onTextChanged: root.actionQuery = text
                            Keys.onPressed: function(event) {
                                const command = event.modifiers & (Qt.ControlModifier | Qt.MetaModifier)
                                if (event.key === Qt.Key_Escape) {
                                    if (text.length) clear(); else root.closeActions()
                                } else if ((event.key === Qt.Key_Left || event.key === Qt.Key_Backspace) && !text.length) root.closeActions()
                                else if (event.key === Qt.Key_K && command) root.closeActions()
                                else if (event.key === Qt.Key_Down) root.selectAction(1)
                                else if (event.key === Qt.Key_Up) root.selectAction(-1)
                                else if (event.key === Qt.Key_Return || event.key === Qt.Key_Enter) root.runAction(root.actionRows[root.actionSelected])
                                else return
                                event.accepted = true
                            }
                        }
                        // Source filter; Tab cycles it from the search field.
                        Rectangle {
                            visible: !root.actionsOpen
                            implicitWidth: kindRow.implicitWidth + Style.space(14)
                            implicitHeight: kindRow.implicitHeight + Style.space(10)
                            radius: Style.cornerRadius
                            color: kindArea.containsMouse ? Style.hoverFill : root.kind === "all" ? "transparent" : Style.selectedAccentFill
                            border.width: Style.spacing.hairline
                            border.color: root.kind === "all" ? root.hairline : Util.alpha(Commons.Color.accent, 0.5)
                            Row {
                                id: kindRow
                                anchors.centerIn: parent
                                spacing: Style.space(8)
                                Text {
                                    anchors.verticalCenter: parent.verticalCenter
                                    text: root.kind === "clipboard" ? "Clipboard" : root.kind === "snippet" ? "Snippets" : "All"
                                    color: root.kind === "all" ? root.dimText : Commons.Color.accent
                                    font.family: Style.font.menuFamily
                                    font.pixelSize: Style.font.body
                                }
                                KeyCap { anchors.verticalCenter: parent.verticalCenter; text: "Tab" }
                            }
                            MouseArea {
                                id: kindArea
                                anchors.fill: parent
                                hoverEnabled: true
                                cursorShape: Qt.PointingHandCursor
                                onClicked: { root.cycleKind(1); search.forceActiveFocus() }
                            }
                        }
                    }
                }

                Divider { visible: !root.editorVisible; Layout.fillWidth: true }

                SbSnippetEditor {
                    id: snippetEditor
                    visible: root.editorVisible
                    Layout.fillWidth: true
                    Layout.fillHeight: true
                    Layout.margins: card.gutter
                    onSaveRequested: function(fields) { root.mutateSnippet("snippet-save", fields) }
                    onDeleteRequested: root.mutateSnippet("snippet-delete", null)
                    onDismissed: function(closePanel) {
                        root.editorVisible = false
                        if (closePanel) root.close()
                        else search.forceActiveFocus()
                    }
                }

                // Body: list on the left, preview (or scratchpad) on the right.
                RowLayout {
                    visible: !root.editorVisible
                    Layout.fillWidth: true
                    Layout.fillHeight: true
                    spacing: 0

                    ColumnLayout {
                        Layout.fillHeight: true
                        Layout.preferredWidth: card.width * 0.55
                        Layout.maximumWidth: card.width * 0.55
                        Layout.topMargin: Style.space(10)
                        Layout.bottomMargin: Style.space(8)
                        Layout.leftMargin: Style.space(8)
                        Layout.rightMargin: Style.space(8)
                        spacing: Style.space(6)

                        Caption {
                            Layout.fillWidth: true
                            Layout.leftMargin: Style.space(10)
                            font.pixelSize: Style.font.bodySmall
                            font.weight: Font.Medium
                            text: root.actionsOpen ? (root.actionTarget ? "Actions" : "General actions")
                                : root.kind === "clipboard" ? "Clipboard history" : root.kind === "snippet" ? "Snippets" : "Clipboard & snippets"
                        }

                        ListView {
                            id: results
                            visible: !root.actionsOpen
                            Layout.fillWidth: true
                            Layout.fillHeight: true
                            clip: true
                            model: displayModel
                            boundsBehavior: Flickable.StopAtBounds
                            spacing: Style.space(2)
                            ScrollBar.vertical: Scrollbar {}
                            // First fully-or-partly visible row, for Ctrl+1–9 hints.
                            readonly property int firstVisible: { contentY; count; return Math.max(0, indexAt(Style.space(20), contentY + 1)) }
                            delegate: ListRow {
                                id: resultRow
                                required property string recordId
                                required property string name
                                required property string kind
                                required property string summary
                                required property int index
                                readonly property int position: index - results.firstVisible + 1
                                width: results.width
                                current: index === root.selected
                                glyph: root.rowGlyph(kind, name)
                                title: name
                                subtitle: kind === "snippet" && summary !== name ? summary : ""
                                onHovered: root.selected = index
                                onActivated: { root.selected = index; root.paste() }
                                onSecondary: { root.selected = index; root.openActions() }
                                KeyCap { visible: resultRow.current; text: "⏎"; lit: true }
                                Text {
                                    visible: !resultRow.current && resultRow.position >= 1 && resultRow.position <= 9
                                    text: "^" + resultRow.position
                                    color: root.faintText
                                    font.family: Style.font.menuFamily
                                    font.pixelSize: Style.font.caption
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
                                    color: root.faintText
                                    font.family: Style.font.menuFamily
                                    font.pixelSize: Style.font.displayLarge
                                }
                                Text {
                                    width: parent.width
                                    horizontalAlignment: Text.AlignHCenter
                                    text: !root.ready ? "Loading…" : search.text.length ? "No matches for “" + search.text + "”" : "Nothing here yet"
                                    textFormat: Text.PlainText
                                    elide: Text.ElideMiddle
                                    color: root.dimText
                                    font.family: Style.font.menuFamily
                                    font.pixelSize: Style.font.title
                                }
                            }
                        }

                        ListView {
                            id: actionList
                            visible: root.actionsOpen
                            Layout.fillWidth: true
                            Layout.fillHeight: true
                            clip: true
                            model: root.actionRows
                            boundsBehavior: Flickable.StopAtBounds
                            spacing: Style.space(2)
                            ScrollBar.vertical: Scrollbar {}
                            delegate: ListRow {
                                id: actionRow
                                required property var modelData
                                required property int index
                                width: actionList.width
                                current: index === root.actionSelected
                                glyph: modelData.glyph
                                title: modelData.label
                                danger: modelData.danger === true
                                onHovered: root.actionSelected = index
                                onActivated: root.runAction(modelData)
                                Repeater {
                                    model: actionRow.current ? ["⏎"] : actionRow.modelData.keys
                                    KeyCap { required property string modelData; text: modelData; lit: actionRow.current }
                                }
                            }
                            Text {
                                anchors.centerIn: parent
                                visible: !root.actionRows.length
                                text: "No matching actions"
                                color: root.dimText
                                font.family: Style.font.menuFamily
                                font.pixelSize: Style.font.title
                            }
                        }
                    }

                    Divider { Layout.fillHeight: true }

                    // Preview: exact text, with stats pinned to the bottom (Alfred).
                    ColumnLayout {
                        visible: !root.scratchVisible
                        Layout.fillWidth: true
                        Layout.fillHeight: true
                        Layout.margins: card.gutter
                        spacing: Style.space(8)
                        ScrollView {
                            Layout.fillWidth: true
                            Layout.fillHeight: true
                            clip: true
                            ScrollBar.vertical: Scrollbar {}
                            TextArea {
                                text: root.previewSubject ? root.preview : ""
                                placeholderText: root.previewSubject ? "" : "Nothing selected"
                                placeholderTextColor: root.faintText
                                readOnly: true
                                textFormat: TextEdit.PlainText
                                wrapMode: TextEdit.WrapAtWordBoundaryOrAnywhere
                                color: root.textColor
                                selectionColor: Style.selectionFill
                                selectedTextColor: Commons.Color.menu.selectedText
                                font.family: Style.font.menuFamily
                                font.pixelSize: Style.font.body
                                padding: 0
                                rightPadding: Style.space(8)
                                background: Item {}
                            }
                        }
                        Caption {
                            visible: root.previewSubject !== null && root.preview.length > 0
                            Layout.fillWidth: true
                            horizontalAlignment: Text.AlignHCenter
                            text: SbActions.stats(root.preview)
                        }
                        Caption {
                            visible: text.length > 0
                            Layout.fillWidth: true
                            horizontalAlignment: Text.AlignHCenter
                            text: !root.previewSubject ? "" : root.previewSubject.kind === "snippet"
                                ? "Snippet" + (root.previewKeyword.length ? " · " + root.previewKeyword : "")
                                : root.previewCreated > 0 ? "Copied " + SbActions.when(root.previewCreated) : "Clipboard"
                        }
                    }

                    // Disposable expansion scratchpad, swapped in for the preview.
                    ColumnLayout {
                        visible: root.scratchVisible
                        Layout.fillWidth: true
                        Layout.fillHeight: true
                        Layout.margins: card.gutter
                        spacing: Style.space(8)
                        RowLayout {
                            Layout.fillWidth: true
                            Caption { text: "Scratchpad"; color: root.textColor; font.pixelSize: Style.font.bodySmall; font.weight: Font.Medium }
                            Caption { Layout.fillWidth: true; horizontalAlignment: Text.AlignRight; text: "Disposable · never an external app" }
                            SbButton { glyph: "󰅖"; hint: "Close scratchpad"; onClicked: { root.scratchVisible = false; root.resetScratch(); search.forceActiveFocus() } }
                        }
                        Input {
                            id: clipboardInput
                            Layout.fillWidth: true
                            maximumLength: 16384
                            placeholderText: "{clipboard} value (not the system clipboard)"
                            onTextChanged: root.resetScratch()
                        }
                        SbButton {
                            Layout.fillWidth: true
                            text: root.expansionEnabled ? "Scratch expansion on" : "Enable scratch expansion"
                            glyph: root.expansionEnabled ? "󰄬" : "󰐊"
                            checked: root.expansionEnabled
                            onClicked: { root.expansionEnabled = !root.expansionEnabled; scratch.forceActiveFocus(); root.resetScratch() }
                        }
                        ScrollView {
                            Layout.fillWidth: true
                            Layout.fillHeight: true
                            clip: true
                            ScrollBar.vertical: Scrollbar {}
                            TextArea {
                                id: scratch
                                placeholderText: "Enable expansion, then type !email, !unicode, !multi, or !vars."
                                textFormat: TextEdit.PlainText
                                wrapMode: TextEdit.WrapAnywhere
                                color: Commons.Color.menu.text
                                placeholderTextColor: root.faintText
                                selectionColor: Style.selectionFill
                                selectedTextColor: Commons.Color.menu.selectedText
                                font.family: Style.font.menuFamily
                                font.pixelSize: Style.font.body
                                padding: Style.space(10)
                                background: Rectangle { color: Style.normalFill; border.color: scratch.activeFocus ? Commons.Color.accent : root.hairline; border.width: 1; radius: Style.cornerRadius }
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
                        SbButton { Layout.fillWidth: true; text: "Insert selected into scratch"; glyph: "󰁅"; enabled: root.activeRow !== null; onClicked: root.insertScratch() }
                    }
                }

                Divider { Layout.fillWidth: true }

                // Footer: status on the left, primary keys on the right.
                RowLayout {
                    Layout.fillWidth: true
                    Layout.leftMargin: card.gutter
                    Layout.rightMargin: card.gutter
                    Layout.topMargin: Style.space(9)
                    Layout.bottomMargin: Style.space(9)
                    spacing: Style.space(10)
                    Caption {
                        Layout.fillWidth: true
                        font.pixelSize: Style.font.bodySmall
                        color: root.faintText
                        text: root.status
                        maximumLineCount: 1
                    }
                    Hint {
                        visible: !root.editorVisible
                        label: root.actionsOpen ? "Run" : "Paste"
                        keys: ["⏎"]
                    }
                    Divider { visible: !root.editorVisible; Layout.preferredHeight: Style.font.title }
                    Hint {
                        visible: !root.editorVisible
                        label: root.actionsOpen ? "Back" : "Actions"
                        keys: root.actionsOpen ? ["←"] : ["→"]
                    }
                }
            }
        }
    }
}
