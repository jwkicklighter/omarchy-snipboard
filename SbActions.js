.pragma library

// Per-item action menu (Right arrow / Ctrl+K). Pure data so it can be tested
// without the shell; Snipboard.qml maps each id to an existing operation.

function forRow(row) {
    const list = []
    if (row) {
        list.push({id: "paste", label: "Paste", glyph: "󰆒", keys: ["⏎"], detail: "Paste into the previously focused app"})
        list.push({id: "copy", label: "Copy to clipboard", glyph: "󰆏", keys: ["Ctrl", "C"], detail: "Copy without pasting"})
        if (row.kind === "snippet") {
            list.push({id: "edit", label: "Edit snippet", glyph: "󰏫", keys: ["Ctrl", "E"], detail: "Change name, text, keyword, or notes"})
            list.push({id: "duplicate", label: "Duplicate snippet", glyph: "󰆑", keys: ["Ctrl", "D"], detail: "Start a new snippet from this one"})
        } else {
            list.push({id: "duplicate", label: "Save as snippet", glyph: "󰅩", keys: ["Ctrl", "S"], detail: "Start a new snippet from this text"})
        }
        list.push({id: "scratch", label: "Insert into scratchpad", glyph: "󰁅", keys: [], detail: "Render into the disposable scratchpad"})
        if (row.kind === "clipboard")
            list.push({id: "delete", label: "Delete from history", glyph: "󰆴", keys: ["Ctrl", "Del"], danger: true, detail: "Remove from history; the system clipboard is unchanged"})
    }
    list.push({id: "new", label: "New snippet", glyph: "󰐕", keys: ["Ctrl", "N"], detail: "Create an empty snippet"})
    list.push({id: "toggle-scratch", label: "Toggle scratchpad", glyph: "󰏫", keys: [], detail: "Swap the preview for the expansion scratchpad"})
    list.push({id: "reload", label: "Reload snippet library", glyph: "󰑐", keys: [], detail: "Adopt the library file from disk"})
    return list
}

// Case-insensitive subsequence match on the label, prefix matches first.
function filter(list, query) {
    const q = String(query || "").toLowerCase()
    if (!q.length) return list
    const scored = []
    for (let i = 0; i < list.length; i++) {
        const label = list[i].label.toLowerCase()
        let at = 0
        for (let c = 0; c < label.length && at < q.length; c++) if (label[c] === q[at]) at++
        if (at === q.length) scored.push({item: list[i], rank: label.indexOf(q) === 0 ? 0 : label.indexOf(q) > 0 ? 1 : 2, order: i})
    }
    scored.sort(function(a, b) { return a.rank - b.rank || a.order - b.order })
    return scored.map(function(s) { return s.item })
}

// "10 words · 76 chars" for the preview footer.
function stats(text) {
    const body = String(text || "")
    const words = body.trim().length ? body.trim().split(/\s+/).length : 0
    const chars = body.replace(/[\uD800-\uDBFF][\uDC00-\uDFFF]/g, "_").length // code points
    const lines = body.length ? body.split("\n").length : 0
    const parts = [words + (words === 1 ? " word" : " words"), chars + (chars === 1 ? " char" : " chars")]
    if (lines > 1) parts.push(lines + " lines")
    return parts.join(" · ")
}

// Clipboard capture time, Alfred-style: "Today 11:14 AM", "Yesterday …", or a date.
function when(seconds, now) {
    if (!(seconds > 0)) return ""
    const date = new Date(seconds * 1000)
    const today = now ? new Date(now) : new Date()
    const startOfToday = new Date(today.getFullYear(), today.getMonth(), today.getDate()).getTime()
    const time = Qt.formatTime(date, "h:mm AP")
    if (date.getTime() >= startOfToday) return "Today " + time
    if (date.getTime() >= startOfToday - 86400000) return "Yesterday " + time
    return Qt.formatDate(date, "MMM d, yyyy") + " " + time
}
