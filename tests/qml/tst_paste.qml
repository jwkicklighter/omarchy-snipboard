import QtQuick
import QtTest
import "../.." as Plugin

TestCase {
    name: "PasteHandoff"
    when: windowShown
    property bool overlayVisible: true
    Plugin.SbPasteHandoff {
        id: handoff
        onHideRequested: overlayVisible = false
        onPasteRequested: verify(!overlayVisible, "Overlay must release focus before paste")
    }
    SignalSpy { id: pastes; target: handoff; signalName: "pasteRequested" }
    function init() {
        handoff.cancel()
        overlayVisible = true
        pastes.clear()
    }
    function test_hide_then_paste_exact_selection_once() {
        handoff.start("snippet-original")
        verify(!overlayVisible)
        compare(pastes.count, 0)
        handoff.start("snippet-other")
        tryCompare(pastes, "count", 1)
        compare(pastes.signalArguments[0][0], "snippet-original")
        verify(!handoff.waiting)
    }
    function test_reopen_cancels_delayed_input() {
        handoff.start("snippet-original")
        handoff.cancel()
        overlayVisible = true
        wait(220)
        compare(pastes.count, 0)
        verify(!handoff.waiting)
    }
    function test_no_selection() {
        handoff.start("")
        verify(overlayVisible)
        verify(!handoff.waiting)
        compare(pastes.count, 0)
    }
}
