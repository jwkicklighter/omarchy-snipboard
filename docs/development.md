# Development and reversible installation

## Installation on this machine

The owner explicitly changed the original placement plan: move the entire repository into the plugin directory, then link it back into `~/Work`. This avoids the installed validator's rejection of a root plugin symlink. The installed loader follows directory symlinks, but the validator's `find ... -type l` rejects the root itself; there is no documented development-link command.

Actual layout (one working tree, including `.git`):

```text
~/.config/omarchy/plugins/io.github.jwkicklighter.snipboard/  real repository
~/Work/omarchy-snipboard -> that repository
```

The ID was checked against the installed catalog; no conflict existed. No existing installation was overwritten. No packaged files or packages were modified.

Repeat the relocation only from an **uninstalled, real** `~/Work/omarchy-snipboard` directory:

```sh
cd ~/Work/omarchy-snipboard
cargo run --release --bin snipboard-tools -- install
```

The script refuses existing destinations, merges, cross-filesystem copying, or replacing an unexpected link. It never invokes shell IPC or enables anything. On this machine this command has **already been run**.

## Build, tests, manifest, and QML lint

The picker uses Quickshell's `PanelWindow` as a transparent layer-shell overlay,
with a compact centered card and no full-screen scrim. It does not create a
normal desktop window or require a Hyprland floating-window rule. Escape and
clicking outside use the same draft-preserving dismissal path.
The temporary Snipboard floating rule from the earlier `FloatingWindow`
implementation has been removed from this machine's Hyprland configuration.

```sh
cd ~/Work/omarchy-snipboard
cargo build --release
cargo test --release
omarchy plugin validate "$HOME/.config/omarchy/plugins/io.github.jwkicklighter.snipboard"
build/release/snipboard-tools validate
build/release/snipboard-tools benchmark --rounds 30
```

Or `make build test validate benchmark`. Cargo's target directory is `build/` (see `.cargo/config.toml`). Tests and benchmarks use disposable owner-only directories under the system temporary directory (`$TMPDIR`, normally `/tmp`), not real prototype data. The validator supplies a temporary `qs` import mapping outside the checkout. Full unsuppressed QML lint output is `${TMPDIR:-/tmp}/snipboard-qmllint.log`.

### Shipping worker versus development builds

Normal Git installations contain no Rust binary and need no Rust toolchain.
QML installs the committed `engine/release.pin.json` into the user's XDG data
directory before starting the worker. Cargo builds/tests use `build/` and do not
override that pin. Smoke-test a candidate with:

```sh
python3 tools/worker_smoke.py build/release/snipboard-worker
```

Use disposable `--data` directories for candidate protocol tests, never live
state. Explicit `--data` keeps all files (including snippets) in that sandbox;
without it, snippets use the plugin's XDG config extension directory and migrate
automatically from the legacy state directory. Do not replace a cached executable with a local build: bootstrap rejects
checksum mismatches. UI integration must use the published engine. Engine source,
publication, and compatible UI/pin are separate PR stages; see
[releases.md](releases.md). `make package` smoke-tests a fresh extracted plugin's
download/install and wire handshake with disposable state; it needs GitHub access.
Python unit tests mock downloads and never contact GitHub or read the clipboard.

QML lint currently exits 0 with installed-metadata warnings (shell singleton tokens declared as `QtObject`, Quickshell's runtime PanelWindow factory, and the QProcess enum). Warnings are reported, not called a clean lint pass. Compilation and runtime checks are separate.

`make test` also runs the snippet editor with `qmltestrunner`, an offscreen software renderer, inert theme fixtures, and no shell/plugin activation. It exercises save shortcuts, exact unchanged text/CRLF, dirty-draft dismissal, delete confirmation, read-only/busy gates, and protocol size checks. It does not access the clipboard or synthesize desktop input. The Rust wrapper (`tests/qml.rs`) skips this suite if Qt Quick Test is unavailable. These are not theme-integration or live-shell compatibility tests.

Large-Unicode editor layout performance is unproven: an attempted offscreen test laying out 65,536 emoji timed out. The current size test checks Unicode counting without shaping that entire fixture. The editor's validation limit is not a UI responsiveness guarantee.

The shell recursively watches the **entire installation**, including docs and build outputs. Writes can reload other non-authentication shell plugins. Avoid building during timing runs: Cargo writes its output under `build/` inside the installation. Git internals are excluded by the host. Do not run `omarchy plugin update` against this development working tree; use ordinary reviewed local git operations.

## Activation — separate, requires approval

The owner approved enabling and opening this overlay only. The clipboard picker and its shortcuts were **not replaced**, and no bar entry or global input hook was added.

It was disabled after validation. Two shell restarts were performed, each separately approved. Disable/re-enable verification observed exactly one helper when enabled and none when disabled; the final state is disabled.

```sh
omarchy-shell shell rescanPlugins
omarchy plugin enable io.github.jwkicklighter.snipboard
omarchy-shell shell summon io.github.jwkicklighter.snipboard '{}'
omarchy-shell shell hide io.github.jwkicklighter.snipboard
```

Rescan is asynchronous; an immediate summon can briefly fail while the host rebuilds its plugin catalog. The shell normally watches QML changes. **Observed loader limitation:** rescan/hot reload can retain an old component at the same URL. Missing new IPC methods demonstrated this. A shell restart loaded the changed code. The owner has granted standing restart permission for this project (see `AGENTS.md`):

```sh
omarchy restart shell
```

No second Quickshell instance is needed or used. QML changes that remain cached cannot be validated just by rescanning.

## Explicit scratch/latency probes — require testing approval

```sh
build/release/snipboard-tools probe scratch
build/release/snipboard-tools probe benchmark
omarchy-shell shell call io.github.jwkicklighter.snipboard metrics ''
```

These hooks edit only the plugin's disposable TextEdit, using fixed synthetic fixtures. They do not produce physical keyboard events, read the system clipboard, copy content, or touch external applications. Scratch expansion starts off each time the panel opens and is turned off after a suite. Diagnostics contain timing arrays and booleans, not input text or payloads. See performance documentation for what the frame signal can and cannot measure.

Build before starting live probes; running the prebuilt `snipboard-tools` binary writes nothing inside the installation (diagnostics go to the temporary directory). After a shell restart, wait for `omarchy-shell shell ping` and plugin initialization before starting a probe; IPC availability alone need not imply plugin readiness.

Manual workflow: open the overlay, press Right (or Ctrl+K) for the action menu, choose **Toggle scratchpad**, enable scratch expansion, then type `!email`, `!unicode`, `!multi`, or `!vars`. `{clipboard}` comes from the explicit field above it. **Insert into scratchpad** is distinct from the external-paste action. Search selection-copy stays ordinary; the **Copy to clipboard** action (Ctrl+C with no search selection) makes result copying explicit.

Picker keys: Up/Down/PageUp/PageDown select, Enter pastes, Ctrl+1–9 pastes by position among the visible rows, Tab/Shift+Tab cycles All/Clipboard/Snippets, and Right at the end of the query (or Ctrl+K, or right-click a row) opens the per-item action menu. In that menu, type to filter, Enter runs, and Left/Backspace on an empty filter, Escape, or Ctrl+K go back. Actions are defined in `SbActions.js` and tested by `tests/qml/tst_actions.qml`.

Theme colors come from Omarchy's `[menu]` surface roles. Secondary text dims `menu.text` instead of using the palette's `muted` role, which is too dark to read on the card in some themes (Tokyo Night `#414868` on `#1a1b26`). `metrics` reports the resolved colors under `colors`.

Snippet editing (after separately approved activation): use the **New snippet** or **Edit snippet** action (Ctrl+N / Ctrl+E). Save with the button or Ctrl+S/Super+S. Name, body/template, optional keyword, and notes stay in prototype `library.json`; nothing is imported from existing snippets. Escape/Cancel preserves a dirty draft until you explicitly discard it. Delete asks for confirmation. Validation/save failures retain the draft. **Reload snippet library** explicitly adopts the current valid file, or retains the previous library read-only on failure; it is unavailable while the editor is open. External changes are detected at save/reload, not watched. This is local editing, not complete folder sync or conflict review.

## Disable and remove without deleting source or data

Disable unloads the overlay/helper and its owned clipboard process. It does not erase prototype data. The built-in clipboard picker needs no restoration because it was never replaced.

```sh
omarchy plugin disable io.github.jwkicklighter.snipboard
```

Uninstall the development placement by **moving the repository back** to Work (not deleting it), then refresh discovery:

```sh
cd "$HOME/.config/omarchy/plugins/io.github.jwkicklighter.snipboard"
build/release/snipboard-tools uninstall
cd "$HOME/Work/omarchy-snipboard"
omarchy-shell shell rescanPlugins
```

Alternatively, to remove **only** the Work shortcut while retaining the repository/installation:

```sh
cd "$HOME/.config/omarchy/plugins/io.github.jwkicklighter.snipboard"
build/release/snipboard-tools unlink-work
```

The latter removes only the verified symlink. Neither operation deletes the project or `${XDG_STATE_HOME:-$HOME/.local/state}/omarchy-snipboard-prototype`. **Do not use `omarchy plugin remove` for this placement:** that generic removal command is not the source-preserving uninstall procedure.
