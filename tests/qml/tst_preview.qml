import QtQuick
import QtTest
import "../.."

TestCase {
    name: "Preview"

    Component {
        id: fixture
        QtObject {
            id: picker
            property var rows: [{id: "a"}, {id: "b"}, {id: "c"}]
            property int selected: 0
            readonly property var activeRow: rows.length > selected ? rows[selected] : null
            property bool actionsOpen: false
            property var actionTarget: null
            readonly property var previewSubject: actionsOpen ? actionTarget : activeRow
            property var requests: []
            property int nextSequence: 0
            property SbPreview preview: SbPreview {
                subject: picker.previewSubject
                onRequested: function(record) {
                    sequence = ++picker.nextSequence
                    picker.requests = picker.requests.concat([{record: record, seq: sequence}])
                }
            }
        }
    }

    function makePicker() {
        const picker = createTemporaryObject(fixture, this)
        verify(picker !== null)
        return picker
    }

    function reply(picker, request, body) {
        picker.preview.receive({seq: request.seq, body: body, keyword: ";" + body, created: 123})
    }

    function test_every_consecutive_selection_refreshes() {
        const picker = makePicker()
        tryCompare(picker, "nextSequence", 1)
        reply(picker, picker.requests[0], "A")
        compare(picker.preview.body, "A")
        for (let i = 1; i <= 6; i++) {
            picker.selected = i % 3
            compare(picker.preview.body, "", "never show the previous row's text")
            compare(picker.preview.keyword, "")
            compare(picker.preview.created, 0)
            tryCompare(picker, "nextSequence", i + 1)
            const request = picker.requests[i]
            compare(request.record, picker.rows[picker.selected].id)
            reply(picker, request, request.record.toUpperCase())
            compare(picker.preview.body, request.record.toUpperCase())
        }
    }

    function test_replacing_rows_at_same_index_refreshes() {
        const picker = makePicker()
        tryCompare(picker, "nextSequence", 1)
        picker.rows = [{id: "replacement"}]
        tryCompare(picker, "nextSequence", 2)
        compare(picker.requests[1].record, "replacement")
        reply(picker, picker.requests[1], "new results")
        compare(picker.preview.body, "new results")
        // Search completion also calls load(); it must not duplicate the request.
        picker.preview.load()
        wait(160)
        compare(picker.nextSequence, 2)
    }

    function test_actions_pin_preview_then_follow_current_selection() {
        const picker = makePicker()
        tryCompare(picker, "nextSequence", 1)
        picker.actionTarget = picker.activeRow
        picker.actionsOpen = true
        picker.selected = 1
        wait(160)
        compare(picker.nextSequence, 1)
        reply(picker, picker.requests[0], "action target")
        compare(picker.preview.body, "action target")
        picker.actionsOpen = false
        tryCompare(picker, "nextSequence", 2)
        compare(picker.requests[1].record, "b")
    }

    function test_debounce_and_return_to_inflight_row_reject_stale_responses() {
        const picker = makePicker()
        tryCompare(picker, "nextSequence", 1)
        const old = picker.requests[0]
        picker.selected = 1
        picker.selected = 2
        picker.selected = 0
        reply(picker, old, "stale A")
        compare(picker.preview.body, "")
        tryCompare(picker, "nextSequence", 2)
        compare(picker.requests[1].record, "a")
        reply(picker, picker.requests[1], "fresh A")
        reply(picker, old, "late stale A")
        compare(picker.preview.body, "fresh A")
        wait(160)
        compare(picker.nextSequence, 2, "rapid movements coalesce")
    }

    function test_empty_results_clear_preview_and_cancel_debounce() {
        const picker = makePicker()
        tryCompare(picker, "nextSequence", 1)
        reply(picker, picker.requests[0], "A")
        picker.selected = 1
        picker.rows = []
        compare(picker.preview.body, "")
        compare(picker.preview.keyword, "")
        compare(picker.preview.created, 0)
        compare(picker.preview.record, "")
        compare(picker.preview.sequence, 0)
        reply(picker, picker.requests[0], "stale")
        wait(160)
        compare(picker.nextSequence, 1)
        compare(picker.preview.body, "")
    }

    function test_clear_cancels_pending_request_and_can_reload_same_subject() {
        const picker = makePicker()
        tryCompare(picker, "nextSequence", 1)
        picker.selected = 1
        picker.preview.clear()
        wait(160)
        compare(picker.nextSequence, 1)
        reply(picker, picker.requests[0], "stale")
        compare(picker.preview.body, "")
        picker.preview.load()
        tryCompare(picker, "nextSequence", 2)
        compare(picker.requests[1].record, "b")
    }
}
