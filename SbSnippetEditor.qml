pragma ComponentBehavior: Bound
import QtQuick
import QtQuick.Controls
import QtQuick.Layouts
import qs.Commons

ColumnLayout {
    id: editor
    property var original: ({id: "", name: "", body: "", keyword: "", notes: ""})
    property var loaded: ({name: "", body: "", keyword: "", notes: ""})
    property string revision: ""
    property bool readOnly: false
    property bool busy: false
    property string error: ""
    property bool confirmDelete: false
    property bool confirmDiscard: false
    property bool dismissPanel: false
    readonly property bool dirty: nameInput.text !== loaded.name || bodyInput.text !== loaded.body || keywordInput.text !== loaded.keyword || notesInput.text !== loaded.notes
    signal saveRequested(var fields)
    signal deleteRequested()
    signal dismissed(bool closePanel)
    spacing: Style.space(8)
    Keys.priority: Keys.BeforeItem
    Keys.onPressed: function(event) {
        if (event.key === Qt.Key_Escape) dismiss(false)
        else if (event.key === Qt.Key_S && (event.modifiers & (Qt.ControlModifier | Qt.MetaModifier))) submit()
        else return
        event.accepted = true
    }

    function codePoints(text) {
        let count = 0
        for (let i = 0; i < text.length; i++, count++) {
            const code = text.charCodeAt(i)
            if (code >= 0xd800 && code <= 0xdbff && i + 1 < text.length) {
                const next = text.charCodeAt(i + 1)
                if (next >= 0xdc00 && next <= 0xdfff) i++
            }
        }
        return count
    }

    function submit() {
        if (busy || readOnly || confirmDelete || confirmDiscard || !nameInput.text.trim().length) return
        // Bound the protocol request before serializing a potentially oversized paste.
        if (bodyInput.text.length > 131072 || notesInput.text.length > 8192 || codePoints(bodyInput.text) > 65536 || codePoints(notesInput.text) > 4096) {
            error = "Body must be at most 65,536 characters; notes at most 4,096. Draft retained."
            return
        }
        // QTextDocument normalizes CRLF/paragraph separators on display. Never
        // rewrite an unchanged field merely because another field was edited.
        saveRequested({name: nameInput.text === loaded.name ? original.name : nameInput.text,
                       body: bodyInput.text === loaded.body ? original.body : bodyInput.text,
                       keyword: keywordInput.text === loaded.keyword ? original.keyword : keywordInput.text,
                       notes: notesInput.text === loaded.notes ? original.notes : notesInput.text})
    }

    function load(snippet, version, locked) {
        original = snippet
        revision = version
        readOnly = locked
        busy = false
        error = ""
        confirmDelete = false
        confirmDiscard = false
        nameInput.text = snippet.name
        bodyInput.text = snippet.body
        keywordInput.text = snippet.keyword
        notesInput.text = snippet.notes
        loaded = {name: nameInput.text, body: bodyInput.text, keyword: keywordInput.text, notes: notesInput.text}
        Qt.callLater(function() { nameInput.forceActiveFocus() })
    }

    function dismiss(closePanel) {
        if (busy) return
        if (dirty) {
            dismissPanel = closePanel
            confirmDiscard = true
            confirmDelete = false
        } else dismissed(closePanel)
    }

    component Field: TextField {
        Layout.fillWidth: true
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
        background: Rectangle {
            color: Style.normalFill
            border.color: parent.activeFocus ? Color.accent : Color.menu.border
            border.width: 1
            radius: Style.cornerRadius
        }
    }

    component Area: TextArea {
        textFormat: TextEdit.PlainText
        wrapMode: TextEdit.WrapAnywhere
        color: Color.menu.text
        placeholderTextColor: Color.muted
        selectionColor: Style.selectionFill
        selectedTextColor: Color.menu.selectedText
        font.family: Style.font.menuFamily
        font.pixelSize: Style.font.body
        padding: Style.space(10)
        background: Rectangle {
            color: Style.normalFill
            border.color: parent.activeFocus ? Color.accent : Color.menu.border
            border.width: 1
            radius: Style.cornerRadius
        }
    }

    component Label: Text {
        color: Color.menu.text
        font.family: Style.font.menuFamily
        font.pixelSize: Style.font.caption
        textFormat: Text.PlainText
    }

    component Hint: Label {
        color: Color.muted
        elide: Text.ElideRight
    }

    // Inline confirmation or notice; urgent tone for destructive/error states.
    component Banner: Rectangle {
        default property alias content: bannerRow.data
        property bool urgent: false
        Layout.fillWidth: true
        implicitHeight: bannerRow.implicitHeight + Style.space(12)
        radius: Style.cornerRadius
        color: Qt.rgba(urgent ? Color.urgent.r : Color.accent.r, urgent ? Color.urgent.g : Color.accent.g, urgent ? Color.urgent.b : Color.accent.b, 0.12)
        border.color: urgent ? Color.urgent : Color.accent
        border.width: 1
        RowLayout {
            id: bannerRow
            anchors.fill: parent
            anchors.leftMargin: Style.space(10)
            anchors.rightMargin: Style.space(6)
            spacing: Style.space(6)
        }
    }

    RowLayout {
        Layout.fillWidth: true
        spacing: Style.space(8)
        Text {
            text: editor.original.id ? "Edit snippet" : "New snippet"
            color: Color.menu.text
            font.family: Style.font.menuFamily
            font.pixelSize: Style.font.title
            font.bold: true
        }
        Hint {
            Layout.fillWidth: true
            text: editor.original.id ? "Identity and usage are preserved" : "Stored in the prototype library only"
        }
        Hint { visible: editor.dirty; text: "● Unsaved"; color: Color.accent }
    }

    RowLayout {
        Layout.fillWidth: true
        spacing: Style.space(10)
        ColumnLayout {
            Layout.fillWidth: true
            spacing: Style.space(4)
            Label { text: "Name" }
            Field { id: nameInput; objectName: "snippetName"; maximumLength: 256; readOnly: editor.busy; placeholderText: "Required" }
        }
        ColumnLayout {
            Layout.fillWidth: false
            Layout.preferredWidth: Style.space(240)
            spacing: Style.space(4)
            Label { text: "Keyword" }
            Field { id: keywordInput; objectName: "snippetKeyword"; maximumLength: 64; readOnly: editor.busy; placeholderText: "Optional, e.g. !email" }
        }
    }

    RowLayout {
        Layout.fillWidth: true
        Label { text: "Body" }
        Hint {
            Layout.fillWidth: true
            horizontalAlignment: Text.AlignRight
            text: editor.loaded.body !== editor.original.body || editor.loaded.notes !== editor.original.notes
                ? "Original line endings kept unless edited; edits use LF"
                : "{date}  {time}  {clipboard}  {cursor}   ·   {{date}} for a literal token"
        }
    }
    ScrollView {
        Layout.fillWidth: true
        Layout.fillHeight: true
        clip: true
        Area { id: bodyInput; objectName: "snippetBody"; readOnly: editor.busy; placeholderText: "Exact text to insert" }
    }

    Label { text: "Notes · searchable, never inserted" }
    ScrollView {
        Layout.fillWidth: true
        Layout.preferredHeight: Style.space(64)
        clip: true
        Area { id: notesInput; objectName: "snippetNotes"; readOnly: editor.busy; placeholderText: "Optional" }
    }

    Banner {
        visible: editor.readOnly || editor.error.length > 0
        urgent: true
        Label {
            Layout.fillWidth: true
            Layout.topMargin: Style.space(6)
            Layout.bottomMargin: Style.space(6)
            wrapMode: Text.Wrap
            color: Color.urgent
            text: editor.error || "Library is read-only. Keep your draft; review storage and reload from the picker."
        }
    }
    Banner {
        visible: editor.confirmDiscard
        Label { Layout.fillWidth: true; text: "Discard unsaved changes?" }
        SbButton { text: "Keep editing"; onClicked: editor.confirmDiscard = false }
        SbButton { objectName: "discardDraft"; text: "Discard"; danger: true; onClicked: editor.dismissed(editor.dismissPanel) }
    }
    Banner {
        visible: editor.confirmDelete
        urgent: true
        Label { Layout.fillWidth: true; text: "Delete this saved snippet? This cannot be undone."; elide: Text.ElideRight }
        SbButton { text: "Keep snippet"; onClicked: editor.confirmDelete = false }
        SbButton { objectName: "confirmDelete"; text: "Delete"; glyph: "󰆴"; danger: true; enabled: !editor.busy && !editor.readOnly; onClicked: editor.deleteRequested() }
    }

    RowLayout {
        Layout.fillWidth: true
        spacing: Style.space(8)
        enabled: !editor.busy && !editor.confirmDiscard && !editor.confirmDelete
        SbButton { objectName: "deleteSnippet"; text: "Delete…"; glyph: "󰆴"; danger: true; visible: editor.original.id.length > 0; enabled: !editor.readOnly; onClicked: editor.confirmDelete = true }
        Hint { Layout.fillWidth: true; horizontalAlignment: Text.AlignRight; text: editor.busy ? "Saving…" : "Ctrl+S save  ·  Esc cancel" }
        SbButton { text: "Cancel"; onClicked: editor.dismiss(false) }
        SbButton {
            objectName: "saveSnippet"
            text: "Save"
            glyph: "󰆓"
            primary: true
            enabled: !editor.readOnly && nameInput.text.trim().length > 0
            onClicked: editor.submit()
        }
    }
}
