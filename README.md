# Omarchy Snipboard

A native Omarchy shell prototype for finding and reusing clipboard text and snippets.

## Status

Working **synthetic-history prototype**, not a first release. Native overlay, combined fuzzy search, separate preview, snippet creation/editing/deletion, usage persistence, and opt-in disposable expansion scratchpad are implemented. The manifest validates; the original picker loaded in the installed shell. The new editor has offscreen tests but still needs live-shell validation.

**Safety limitation:** Enter/result clicks refuse external paste with an explicit copy-only fallback. Global typed expansion is unavailable, not merely hidden behind an enable switch. No external context has a verified safe insertion adapter. Do not use this as a replacement clipboard picker yet.

The scratchpad proves `!email`, Unicode, multiline insertion, and `{date}`, `{time}`, `{clipboard}`, `{cursor}` inside its own plain TextEdit. `{clipboard}` currently uses an explicit scratch value, never the existing system clipboard. Copy-only uses `wl-copy`; external copy/paste compatibility remains unverified.

## Build and test

```sh
cd ~/Work/omarchy-snipboard
make build
make test
make validate
make benchmark
```

Uses an installed Rust toolchain (cargo, Rust 1.89 or newer), Quickshell, wl-clipboard, and `setpriv` (util-linux); layout tests link libxkbcommon, and offscreen editor tests use Qt Quick Test's `qmltestrunner` (skipped if unavailable). Nothing installs system packages; the first build downloads the crates pinned in `Cargo.lock`. The helper (search, storage, templates) is a single Rust binary running as a separate persistent process, never inside the shell process. Build it before activation: the overlay launches `build/release/snipboard-worker`.

## Development installation

At the owner's request, the **entire git repository** is in:

`~/.config/omarchy/plugins/io.github.jwkicklighter.snipboard/`

`~/Work/omarchy-snipboard` is a symlink to that same directory; there is no second source copy. Activation is separate from installation. See [exact install, reload, disable, and unlink commands](docs/development.md).

The overlay was enabled with approval for validation, then **left disabled** after cleanup checks. This development increment does not enable it or change desktop settings. Existing clipboard configuration was not replaced or re-enabled. Backend and offscreen editor tests cover local editing; external application compatibility and the UI latency targets remain open blockers.

Prototype data is separate, in `${XDG_STATE_HOME:-$HOME/.local/state}/omarchy-snipboard-prototype/`, with owner-only files. Existing clipboard history, snippets, and capture ownership are untouched.

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

The feature-goals document describes intended product behavior. The prototype documents distinguish implemented, tested, and blocked behavior. Full sync, import/export UI, history capture, rich previews, OCR, QR decoding, and automatic expansion undo are not implemented.

## License

[MIT](LICENSE).
