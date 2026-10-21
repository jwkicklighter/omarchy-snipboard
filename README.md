# Omarchy Snipboard

[![Build and test](https://github.com/jwkicklighter/omarchy-snipboard/actions/workflows/ci.yml/badge.svg)](https://github.com/jwkicklighter/omarchy-snipboard/actions/workflows/ci.yml)

A native Omarchy shell prototype for finding and reusing clipboard text and snippets.

![Snipboard on the Omarchy desktop, showing sample clipboard history and snippets](preview.png)

*Full-screen preview with fictional sample data.*

Super+Shift+V opens a compact, centered picker on a transparent shell overlay,
without painting over or dimming the surrounding desktop. Esc or clicking outside dismisses it. No Hyprland floating-window rule is required.

The picker is a single search line over a result list, with a preview of the
selection on the right showing word/character counts and when it was copied.
Enter pastes, Ctrl+1–9 pastes by position, and Tab cycles the source filter.
Right arrow (or Ctrl+K) opens the actions for the selected item: copy, save
as snippet, edit/duplicate, insert into scratchpad, delete from history, and
more. Colors follow the active Omarchy theme's menu roles.

## Status

Working **text-history prototype**, not a first release. Native overlay, system clipboard text capture, combined fuzzy search, separate preview, snippet creation/editing/duplication/deletion, save-result-as-snippet, usage persistence, and opt-in disposable expansion scratchpad are implemented. The manifest validates; the original picker loaded in the installed shell. The new editor and clipboard watcher still need live-shell validation.

Enter/result clicks paste the rendered selection into the previously focused window: the overlay closes, waits 150 ms for focus restoration, then the worker sets the clipboard and uses Hyprland's compositor key-state API, like Omarchy's universal Super+V. Normal apps receive Ctrl+V; apps tagged as terminals receive Ctrl+Shift+V. The latter reads the regular clipboard in Ghostty even when Shift+Insert reads the separate primary selection. Explicit modifier state avoids merging physically held modifiers into a virtual keyboard chord. Clipboard and input failures reopen the picker with an error. Reopening during the focus delay cancels the pending paste. This replaces clipboard contents. `{cursor}` moves the caret back from the end using unmodified Left keys after a short paste-delivery delay; navigation is skipped if the focused window changes. This is best-effort in external apps: slow paste handling, terminal multiline editing, bidirectional text, or app-specific caret rules can differ. Global typed expansion is unavailable: typing a keyword in another app currently has no effect.

Copy and Paste resolve `{clipboard}` from the current regular system clipboard before replacing it, preserving newlines and treating its contents literally. A missing/unreadable text selection reports an error rather than silently inserting an empty value. Copy-only removes `{cursor}` without moving any caret. The disposable scratchpad uses its explicitly labeled test clipboard field instead, and positions `{cursor}` directly in its own plain TextEdit. External application compatibility still needs live validation.

With search focused, Ctrl/Super+N opens a new snippet, Ctrl/Super+E edits a selected snippet, and Ctrl/Super+D duplicates a snippet or saves a clipboard result as a new snippet. Ctrl/Super+Delete (or the trash button) immediately deletes the selected clipboard entry from local history, without changing snippets or the system clipboard. Deletion persists across restarts; copying the same text again can record it again. Matching buttons offer the same actions. Prefilled drafts require an explicit Save; duplicates clear the keyword and start with independent usage. Clipboard results are reused literally; saving them as snippets escapes recognized variable tokens.

While the plugin is loaded (including when the picker is closed), `wl-paste --type text --watch` records the regular Wayland clipboard. Exact UTF-8 text up to 64 KiB is accepted; empty, binary/NUL-containing, oversized, and selections marked sensitive by wl-paste are skipped. Sensitive detection depends on the source application's hints, not universal password-field detection. Duplicates move to the top rather than creating extra entries. History persists locally in owner-only `history.local.json`, separately from snippets, with a 1,000-item / 30-day / 4 MiB text budget (also subject to the 8 MiB encoded-file limit). Retention runs on load and capture. Images, pause/pin controls, configurable retention, and importing another clipboard manager's history are not implemented. Unload/disable stops capture; reload retries a failed watcher. This does not claim exclusive capture ownership or modify other clipboard services.

## Development build and test

For installation, skip to [Install, update, and remove](#install-update-and-remove).

```sh
cd ~/Work/omarchy-snipboard
make build
make test
make validate
make benchmark
```

Development uses Rust 1.89 or newer; **normal installation does not need Rust or Cargo**. Layout tests link libxkbcommon, offscreen editor/handoff tests use Qt Quick Test's `qmltestrunner`, and compositor script tests use standalone `lua` (those optional suites skip locally if their runner is unavailable, but are required in CI). Python 3.11+ and binutils (`readelf`) run release checks and packaging tools. Nothing installs system packages; development builds download the crates pinned in `Cargo.lock`. The helper (search, storage, templates) is a single Rust binary running as a separate persistent process, never inside the shell process. The overlay launches the checked-in `bin/linux-x86_64/snipboard-worker`; `cargo build` alone does not replace that shipping binary. See [development instructions](docs/development.md) for updating it.

## Install, update, and remove

Requires **Omarchy Quattro's plugin-capable shell**, its Qt 6/Quickshell runtime,
Hyprland with `hyprctl eval`, `wl-copy`/`wl-paste` from wl-clipboard, and
`setpriv` from util-linux. The prebuilt worker supports Linux x86_64 with glibc
2.39 or newer (built on Ubuntu 24.04). Other architectures are not currently supported.
Install any missing dependencies yourself before enabling the plugin.

Review the code first, then install and enable with one command. The repository
includes the compiled worker: **no Rust toolchain, build step, or separate binary
download is needed**.

```sh
omarchy plugin add https://github.com/jwkicklighter/omarchy-snipboard --enable
```

Enabling starts recording regular clipboard text, including potentially private
text unless the source marks it sensitive. Data stays in owner-only local files;
there is no telemetry or upload. Plugins are **unsandboxed code**. The worker's
process separation is not a security sandbox. No shortcuts or other clipboard
services are changed by installation; Super+Shift+V is this machine's development
binding, not an automatically installed global shortcut.

Alternatively, download the versioned Linux x86_64 `.tar.gz` bundle and its
`.sha256` file from [GitHub Releases](https://github.com/jwkicklighter/omarchy-snipboard/releases).
In the download directory, verify the checksum (substitute the release version):

```sh
sha256sum --check snipboard-v0.1.0-linux-x86_64.tar.gz.sha256
tar -tzf snipboard-v0.1.0-linux-x86_64.tar.gz
```

The bundle contains QML, documentation, license, metadata, and the prebuilt worker
at its expected path. Extract it into `~/.config/omarchy/plugins/` **only when
`io.github.jwkicklighter.snipboard/` does not already exist**. Then validate,
enable using `omarchy plugin enable io.github.jwkicklighter.snipboard`, and
restart with `omarchy restart shell`. A checksum detects corruption; it is not an
independent signature or marketplace security approval.

For a Git installation update, run
`omarchy plugin update io.github.jwkicklighter.snipboard`; the updated worker
arrives with the repository, with no rebuild needed. If the shell retains old
QML, run `omarchy restart shell`. For a bundle installation, disable and move the old installation out of
the plugins directory before extracting a replacement; do not overlay versions.
Omarchy's git update command does not update extracted bundles. Marketplace
verification covers an exact commit; standard installation/update clones mutable
upstream HEAD, which may differ from that verified snapshot.

To stop recording and remove a **normal user installation**:

```sh
omarchy plugin disable io.github.jwkicklighter.snipboard
omarchy plugin remove io.github.jwkicklighter.snipboard
```

Local history and snippets remain under
`${XDG_STATE_HOME:-$HOME/.local/state}/omarchy-snipboard-prototype/`.
Remove that directory separately only if you want to permanently erase its data.
For this owner's live development checkout, use the source-preserving
[development uninstall procedure](docs/development.md) instead.

## Releases and marketplace

GitHub Actions builds/tests every push and pull request, and checks the latest
stable Rust weekly alongside the minimum supported Rust. Release Please maintains
a release PR from Conventional Commits. Automation builds and commits the worker
to that PR before enabling auto-merge. Merging it automatically tags, tests,
and publishes a bundle containing that exact worker and a SHA-256 checksum to GitHub Releases.
Optional automation opens an exact-commit marketplace update request after a
release; **marketplace maintainers still must approve publication**.
See [release and marketplace setup](docs/releases.md) for the initial submission,
token configuration, and release procedure.

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
