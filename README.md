# Omarchy Snipboard

A native Omarchy shell prototype for finding and reusing clipboard text and snippets.

Super+Shift+V opens a compact, centered picker on a transparent shell overlay,
without painting over or dimming the surrounding desktop. Esc, the close button,
or clicking outside dismisses it. No Hyprland floating-window rule is required.

## Status

Working **text-history prototype**, not a first release. Native overlay, system clipboard text capture, combined fuzzy search, separate preview, snippet creation/editing/duplication/deletion, save-result-as-snippet, usage persistence, and opt-in disposable expansion scratchpad are implemented. The manifest validates; the original picker loaded in the installed shell. The new editor and clipboard watcher still need live-shell validation.

Enter/result clicks paste the rendered selection into the previously focused window: the overlay closes, waits 150 ms for focus restoration, then the worker sets the clipboard and uses Hyprland's compositor key-state API, like Omarchy's universal Super+V. Normal apps receive Ctrl+V; apps tagged as terminals receive Ctrl+Shift+V. The latter reads the regular clipboard in Ghostty even when Shift+Insert reads the separate primary selection. Explicit modifier state avoids merging physically held modifiers into a virtual keyboard chord. Clipboard and input failures reopen the picker with an error. Reopening during the focus delay cancels the pending paste. This replaces clipboard contents and leaves the caret at the end (`{cursor}` is removed but external caret positioning is not implemented). Global typed expansion is unavailable: typing a keyword in another app currently has no effect.

The scratchpad proves `!email`, Unicode, multiline insertion, and `{date}`, `{time}`, `{clipboard}`, `{cursor}` inside its own plain TextEdit. `{clipboard}` currently uses an explicit scratch value, never the existing system clipboard. Copy-only uses `wl-copy`; external copy/paste compatibility remains unverified.

With search focused, Ctrl/Super+N opens a new snippet, Ctrl/Super+E edits a selected snippet, and Ctrl/Super+D duplicates a snippet or saves a clipboard result as a new snippet. Ctrl/Super+Delete (or the trash button) immediately deletes the selected clipboard entry from local history, without changing snippets or the system clipboard. Deletion persists across restarts; copying the same text again can record it again. Matching buttons offer the same actions. Prefilled drafts require an explicit Save; duplicates clear the keyword and start with independent usage. Clipboard results are reused literally; saving them as snippets escapes recognized variable tokens.

While the plugin is loaded (including when the picker is closed), `wl-paste --type text --watch` records the regular Wayland clipboard. Exact UTF-8 text up to 64 KiB is accepted; empty, binary/NUL-containing, oversized, and selections marked sensitive by wl-paste are skipped. Sensitive detection depends on the source application's hints, not universal password-field detection. Duplicates move to the top rather than creating extra entries. History persists locally in owner-only `history.local.json`, separately from snippets, with a 1,000-item / 30-day / 4 MiB text budget (also subject to the 8 MiB encoded-file limit). Retention runs on load and capture. Images, pause/pin controls, configurable retention, and importing another clipboard manager's history are not implemented. Unload/disable stops capture; reload retries a failed watcher. This does not claim exclusive capture ownership or modify other clipboard services.

## Build and test

```sh
cd ~/Work/omarchy-snipboard
make build
make test
make validate
make benchmark
```

Uses an installed Rust toolchain (cargo, Rust 1.89 or newer), Quickshell, wl-clipboard, Hyprland's Lua API (`hyprctl eval`), and `setpriv` (util-linux); layout tests link libxkbcommon, offscreen editor/handoff tests use Qt Quick Test's `qmltestrunner`, and compositor script tests use standalone `lua` (those optional suites skip if their runner is unavailable). Nothing installs system packages; the first build downloads the crates pinned in `Cargo.lock`. The helper (search, storage, templates) is a single Rust binary running as a separate persistent process, never inside the shell process. Build it before activation: the overlay launches `build/release/snipboard-worker`.

## Development installation

At the owner's request, the **entire git repository** is in:

`~/.config/omarchy/plugins/io.github.jwkicklighter.snipboard/`

`~/Work/omarchy-snipboard` is a symlink to that same directory; there is no second source copy. Activation is separate from installation. See [exact install, reload, disable, and unlink commands](docs/development.md).

The overlay was enabled with approval for validation, then **left disabled** after cleanup checks. This development increment does not enable it or change desktop settings. Existing clipboard configuration was not replaced or re-enabled. Backend and offscreen editor tests cover local editing; external application compatibility and the UI latency targets remain open blockers.

Prototype data is separate, in `${XDG_STATE_HOME:-$HOME/.local/state}/omarchy-snipboard-prototype/`, with owner-only files. Other clipboard managers' history, snippets, and services are untouched. The initial snippet library still contains generated examples; synthetic clipboard entries are no longer loaded.

## Goals

- Work equally well with a keyboard or mouse.
- Offer a polished, compact interface using the active Omarchy theme colors.
- Search clipboard history and custom snippets without interrupting your typing.
- Preview text, images, links, files, and other clipboard content appropriately.
- Remember snippet usage and favor frequently, recently used matches.
- Optionally expand a customizable keyword such as `!email` into a snippet while typing in another app.
- Support date/time, clipboard-text, and cursor-position variables, plus a user-managed sync folder for snippets and portable settings.
- Keep personal content local and provide clear controls over recording and expansion.

## Project documentation

- [Feature goals and acceptance criteria](docs/feature-goals.md)
- [Implementation roadmap and open decisions](docs/roadmap.md)
- [Confirmed product decisions](docs/decisions.md)
- [Development baseline and compatibility matrix](docs/compatibility.md)
- [Development and reversible installation](docs/development.md)
- [Prototype boundary and safety design](docs/prototype.md)
- [Measured performance and limitations](docs/performance.md)

The feature-goals document describes intended product behavior. The prototype documents distinguish implemented, tested, and blocked behavior. Full sync, import/export UI, image history, rich previews, OCR, QR decoding, and automatic expansion undo are not implemented.

## License

[MIT](LICENSE).
