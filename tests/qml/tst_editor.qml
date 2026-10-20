import QtQuick
import QtTest
import "../.." as Plugin

TestCase {
    id: tests
    name: "SnippetEditor"
    when: windowShown
    visible: true
    width: 868
    height: 480
    Plugin.SbSnippetEditor { id: editor; anchors.fill: parent }
    SignalSpy { id: saves; target: editor; signalName: "saveRequested" }
    SignalSpy { id: deletes; target: editor; signalName: "deleteRequested" }
    SignalSpy { id: dismissals; target: editor; signalName: "dismissed" }

    function field(name) { return findChild(editor, name) }
    function fixture() { return {id: "snippet-fixture", name: "Fixture", body: "\t🌍\n trailing \n{cursor}", keyword: "!fixture", notes: "Searchable\nnotes"} }
    function init() {
        field("snippetBody").wrapMode = TextEdit.WrapAnywhere
        editor.load(fixture(), "revision-fixture", false)
        saves.clear()
        deletes.clear()
        dismissals.clear()
        wait(0)
    }

    function test_exact_body_and_save_shortcut() {
        compare(editor.dirty, false)
        compare(field("snippetBody").text, fixture().body)
        field("snippetName").text = "Renamed"
        compare(editor.dirty, true)
        field("snippetBody").forceActiveFocus()
        keyClick(Qt.Key_S, Qt.ControlModifier)
        compare(saves.count, 1)
        compare(saves.signalArguments[0][0].body, fixture().body)
        compare(saves.signalArguments[0][0].notes, fixture().notes)
        compare(saves.signalArguments[0][0].name, "Renamed")
    }

    function test_theme_colors_resolve_inside_controls() {
        compare(field("snippetName").color.toString(), "#eeeeee")
        compare(field("snippetBody").color.toString(), "#eeeeee")
        field("snippetName").forceActiveFocus()
        compare(field("snippetName").background.border.color.toString(), "#aaaaaa")
        compare(field("discardDraft").tone.toString(), "#a55555")
    }

    function test_line_endings_and_literal_markup() {
        const snippet = fixture()
        snippet.body = "<b>literal</b>\r\n\tline\r\nend \n"
        snippet.notes = "first\r\nsecond"
        editor.load(snippet, "revision", false)
        compare(editor.dirty, false)
        editor.submit()
        compare(saves.count, 1)
        compare(saves.signalArguments[0][0].body, snippet.body)
        compare(saves.signalArguments[0][0].notes, snippet.notes)
        field("snippetName").text = "Rename only"
        editor.submit()
        compare(saves.signalArguments[1][0].body, snippet.body)
        compare(saves.signalArguments[1][0].notes, snippet.notes)
    }

    function test_escape_and_discard_confirmation() {
        field("snippetBody").text += " changed"
        field("snippetBody").forceActiveFocus()
        keyClick(Qt.Key_Escape)
        verify(editor.confirmDiscard)
        compare(dismissals.count, 0)
        mouseClick(field("discardDraft"))
        compare(dismissals.count, 1)
        compare(dismissals.signalArguments[0][0], false)
    }

    function test_clean_dismiss_and_busy_draft() {
        editor.dismiss(true)
        compare(dismissals.count, 1)
        compare(dismissals.signalArguments[0][0], true)
        editor.busy = true
        editor.dismiss(false)
        editor.submit()
        compare(dismissals.count, 1)
        compare(saves.count, 0)
        verify(field("snippetBody").readOnly)
    }

    function test_readonly_and_delete_confirmation() {
        editor.readOnly = true
        editor.submit()
        compare(saves.count, 0)
        verify(!field("deleteSnippet").enabled)
        editor.readOnly = false
        mouseClick(field("deleteSnippet"))
        verify(editor.confirmDelete)
        compare(deletes.count, 0)
        editor.submit()
        compare(saves.count, 0)
        mouseClick(field("confirmDelete"))
        compare(deletes.count, 1)
    }

    function test_bounds_before_protocol_and_unicode_counts() {
        // Avoid benchmarking line layout while checking protocol bounds.
        field("snippetBody").wrapMode = TextEdit.NoWrap
        field("snippetBody").text = "x".repeat(65537)
        editor.submit()
        compare(saves.count, 0)
        verify(editor.error.length > 0)
        compare(field("snippetBody").text.length, 65537)
        compare(editor.codePoints("🌍".repeat(65536)), 65536)
        field("snippetBody").text = "🌍 exact\n body"
        editor.submit()
        compare(saves.count, 1)
        field("snippetNotes").text = "x".repeat(4097)
        editor.submit()
        compare(saves.count, 1)
    }

    function test_new_snippet_and_nonblank_name() {
        editor.load({id: "", name: "", body: "", notes: "", keyword: ""}, "revision", false)
        verify(!field("deleteSnippet").visible)
        field("snippetName").text = "  "
        editor.submit()
        compare(saves.count, 0)
        field("snippetName").text = "New fixture"
        editor.submit()
        compare(saves.count, 1)
        compare(saves.signalArguments[0][0].body, "")
    }

    function test_prefilled_draft_requires_explicit_save_or_discard() {
        const draft = {id: "", name: "Fixture (copy)", body: "🌍\r\n{date}", notes: "notes", keyword: ""}
        editor.load(draft, "revision", false, true)
        verify(editor.dirty)
        verify(!field("deleteSnippet").visible)
        editor.dismiss(true)
        verify(editor.confirmDiscard)
        compare(dismissals.count, 0)
        editor.confirmDiscard = false
        editor.submit()
        compare(saves.count, 1)
        compare(saves.signalArguments[0][0].body, draft.body)
        compare(saves.signalArguments[0][0].keyword, "")
        editor.load(fixture(), "revision", false)
        verify(!editor.dirty)
    }
}
