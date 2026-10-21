import QtQuick
import QtTest
import "../../SbActions.js" as SbActions

TestCase {
    name: "Actions"

    function ids(list) { return list.map(function(a) { return a.id }) }

    function test_clipboard_row_offers_history_actions() {
        const list = ids(SbActions.forRow({id: "clipboard-1", kind: "clipboard"}))
        compare(list[0], "paste", "Enter's action comes first")
        verify(list.indexOf("delete") >= 0)
        verify(list.indexOf("duplicate") >= 0)
        verify(list.indexOf("edit") < 0, "clipboard text is not an editable snippet")
    }

    function test_snippet_row_offers_editing_not_deletion() {
        const list = ids(SbActions.forRow({id: "snippet-1", kind: "snippet"}))
        verify(list.indexOf("edit") >= 0)
        verify(list.indexOf("delete") < 0, "snippets are deleted from the editor with confirmation")
    }

    function test_no_row_offers_only_general_actions() {
        compare(ids(SbActions.forRow(null)), ["new", "toggle-scratch", "reload"])
    }

    function test_save_and_duplicate_shortcuts() {
        const save = SbActions.forRow({id: "clipboard-1", kind: "clipboard"}).filter(function(a) { return a.id === "duplicate" })[0]
        compare(save.label, "Save as snippet")
        compare(save.keys, ["Ctrl", "S"])
        const duplicate = SbActions.forRow({id: "snippet-1", kind: "snippet"}).filter(function(a) { return a.id === "duplicate" })[0]
        compare(duplicate.keys, ["Ctrl", "D"])
    }

    function test_filter_prefers_prefix_then_subsequence() {
        const list = SbActions.forRow({id: "clipboard-1", kind: "clipboard"})
        compare(SbActions.filter(list, "").length, list.length)
        compare(SbActions.filter(list, "del")[0].id, "delete")
        compare(SbActions.filter(list, "COPY")[0].id, "copy")
        compare(ids(SbActions.filter(list, "snip")), ["duplicate", "new", "reload"])
        compare(SbActions.filter(list, "zzz").length, 0)
    }

    function test_stats() {
        compare(SbActions.stats(""), "0 words · 0 chars")
        compare(SbActions.stats("one"), "1 word · 3 chars")
        compare(SbActions.stats("a b\nc 🌍"), "4 words · 7 chars · 2 lines")
    }

    function test_when() {
        const now = new Date(2026, 9, 20, 15, 0).getTime()
        compare(SbActions.when(0, now), "")
        verify(SbActions.when(new Date(2026, 9, 20, 11, 14).getTime() / 1000, now).indexOf("Today ") === 0)
        verify(SbActions.when(new Date(2026, 9, 19, 23, 0).getTime() / 1000, now).indexOf("Yesterday ") === 0)
        verify(SbActions.when(new Date(2026, 8, 1, 9, 0).getTime() / 1000, now).indexOf("Sep 1, 2026") === 0)
    }

    function test_edge_whitespace_marks_only_leading_and_trailing_runs() {
        compare(SbActions.edgeWhitespace("a b", 0), [])
        compare(SbActions.edgeWhitespace("", 0), [])
        compare(SbActions.edgeWhitespace(" \ta b \n", 0),
                [{at: 0, glyph: "·"}, {at: 1, glyph: "→"}, {at: 5, glyph: "·"}, {at: 6, glyph: "↵"}])
        compare(SbActions.edgeWhitespace("x\r\n", 0), [{at: 2, glyph: "↵"}], "CR has no glyph of its own")
        compare(SbActions.edgeWhitespace("\u00a0x", 0), [{at: 0, glyph: "°"}])
        compare(SbActions.edgeWhitespace("   ", 0).length, 3, "all-whitespace is marked once")
        compare(SbActions.edgeWhitespace("     x     ", 2).map(function(m) { return m.at }), [0, 1, 9, 10])
    }
}
