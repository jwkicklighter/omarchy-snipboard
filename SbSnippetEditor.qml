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
        background: Rectangle {
            color: Style.normalFill
            border.color: parent.activeFocus ? Style.focusBorderColor : Color.menu.border
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

    Label { text: editor.original.id ? "Edit snippet · identity and usage are preserved" : "New snippet · stored in the prototype library only" }
    Label { text: "Name (required)" }
    Field { id: nameInput; objectName: "snippetName"; maximumLength: 256; readOnly: editor.busy; placeholderText: "Snippet name" }
    RowLayout {
        Layout.fillWidth: true
        Label { text: "Keyword (optional)" }
        Field { id: keywordInput; objectName: "snippetKeyword"; maximumLength: 64; readOnly: editor.busy; placeholderText: "Unique, case-sensitive, no prefix conflicts" }
    }
    Label { text: "Exact body · {date} {time} {clipboard} {cursor} · {{date}} escapes a token" }
    Label {
        visible: editor.loaded.body !== editor.original.body || editor.loaded.notes !== editor.original.notes
        text: "Original line endings preserved unless edited; edited fields use LF newlines."
    }
    ScrollView {
        Layout.fillWidth: true
        Layout.fillHeight: true
        clip: true
        TextArea {
            id: bodyInput
            objectName: "snippetBody"
            readOnly: editor.busy
            textFormat: TextEdit.PlainText
            wrapMode: TextEdit.WrapAnywhere
            color: Color.menu.text
            selectionColor: Style.selectionFill
            selectedTextColor: Color.menu.selectedText
            font.family: Style.font.menuFamily
            font.pixelSize: Style.font.body
            background: Rectangle { color: Style.normalFill; border.color: bodyInput.activeFocus ? Style.focusBorderColor : Color.menu.border; border.width: 1 }
        }
    }
    Label { text: "Searchable notes (never inserted)" }
    ScrollView {
        Layout.fillWidth: true
        Layout.preferredHeight: Style.space(56)
        clip: true
        TextArea {
            id: notesInput
            objectName: "snippetNotes"
            readOnly: editor.busy
            textFormat: TextEdit.PlainText
            wrapMode: TextEdit.WrapAnywhere
            color: Color.menu.text
            selectionColor: Style.selectionFill
            selectedTextColor: Color.menu.selectedText
            font.family: Style.font.menuFamily
            font.pixelSize: Style.font.body
            background: Rectangle { color: Style.normalFill; border.color: notesInput.activeFocus ? Style.focusBorderColor : Color.menu.border; border.width: 1 }
        }
    }
    Label {
        Layout.fillWidth: true
        visible: editor.readOnly || editor.error.length > 0
        wrapMode: Text.Wrap
        text: editor.error || "Library is read-only. Keep your draft; review storage and reload from the picker."
    }
    RowLayout {
        visible: editor.confirmDiscard
        Label { text: "Discard unsaved changes?" }
        SbButton { text: "Keep editing"; onClicked: editor.confirmDiscard = false }
        SbButton { objectName: "discardDraft"; text: "Discard"; onClicked: editor.dismissed(editor.dismissPanel) }
    }
    RowLayout {
        visible: editor.confirmDelete
        Label { text: "Delete this saved snippet? This cannot be undone." }
        SbButton { text: "Keep snippet"; onClicked: editor.confirmDelete = false }
        SbButton { objectName: "confirmDelete"; text: "Confirm delete"; enabled: !editor.busy && !editor.readOnly; onClicked: editor.deleteRequested() }
    }
    RowLayout {
        Layout.fillWidth: true
        enabled: !editor.busy && !editor.confirmDiscard && !editor.confirmDelete
        SbButton {
            objectName: "saveSnippet"
            text: "Save"
            enabled: !editor.readOnly && nameInput.text.trim().length > 0
            onClicked: editor.submit()
        }
        SbButton { text: "Cancel"; onClicked: editor.dismiss(false) }
        Item { Layout.fillWidth: true }
        SbButton { objectName: "deleteSnippet"; text: "Delete…"; visible: editor.original.id.length > 0; enabled: !editor.readOnly; onClicked: editor.confirmDelete = true }
    }
}
