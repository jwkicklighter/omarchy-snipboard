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
python -m tools.development install
```

The script refuses existing destinations, merges, cross-filesystem copying, or replacing an unexpected link. It never invokes shell IPC or enables anything. On this machine this command has **already been run**.

## Build, tests, manifest, and QML lint

```sh
cd ~/Work/omarchy-snipboard
cmake -S . -B build -DCMAKE_BUILD_TYPE=Release
cmake --build build --parallel 2
python -m unittest discover -s tests -v
omarchy plugin validate "$HOME/.config/omarchy/plugins/io.github.jwkicklighter.snipboard"
python -m tools.validate
python -m tools.benchmark --rounds 30
```

Or `make build test validate benchmark`. Tests and benchmarks use disposable directories under `/tmp/opencode`, not real prototype data. The validator helper supplies a temporary `qs` import mapping outside the checkout. Full unsuppressed QML lint output is `/tmp/opencode/snipboard-qmllint.log`.

QML lint currently exits 0 with installed-metadata warnings (shell singleton tokens declared as `QtObject`, Quickshell's runtime PanelWindow factory, and the QProcess enum). Warnings are reported, not called a clean lint pass. Compilation and runtime checks are separate.

`make test` also runs the snippet editor with `qmltestrunner`, an offscreen software renderer, inert theme fixtures, and no shell/plugin activation. It exercises save shortcuts, exact unchanged text/CRLF, dirty-draft dismissal, delete confirmation, read-only/busy gates, and protocol size checks. It does not access the clipboard or synthesize desktop input. The Python wrapper skips this suite if Qt Quick Test is unavailable. These are not theme-integration or live-shell compatibility tests.

Large-Unicode editor layout performance is unproven: an attempted offscreen test laying out 65,536 emoji timed out. The current size test checks Unicode counting without shaping that entire fixture. The editor's validation limit is not a UI responsiveness guarantee.

The shell recursively watches the **entire installation**, including docs and build outputs. Writes can reload other non-authentication shell plugins. Avoid building during timing runs. Imports disable bytecode generation to limit watcher feedback. Git internals are excluded by the host. Do not run `omarchy plugin update` against this development working tree; use ordinary reviewed local git operations.

## Activation — separate, requires approval

The owner approved enabling and opening this overlay only. The clipboard picker and its shortcuts were **not replaced**, and no bar entry or global input hook was added.

It was disabled after validation. Two shell restarts were performed, each separately approved. Disable/re-enable verification observed exactly one helper when enabled and none when disabled; the final state is disabled.

```sh
omarchy-shell shell rescanPlugins
omarchy plugin enable io.github.jwkicklighter.snipboard
omarchy-shell shell summon io.github.jwkicklighter.snipboard '{}'
omarchy-shell shell hide io.github.jwkicklighter.snipboard
```

Rescan is asynchronous; an immediate summon can briefly fail while the host rebuilds its plugin catalog. The shell normally watches QML changes. **Observed loader limitation:** rescan/hot reload can retain an old component at the same URL. Missing new IPC methods demonstrated this. A shell restart loaded the changed code; never automate a restart without approval:

```sh
omarchy restart shell
```

No second Quickshell instance is needed or used. QML changes that remain cached cannot be validated just by rescanning.

## Explicit scratch/latency probes — require testing approval

```sh
python -B -m tools.probe scratch
python -B -m tools.probe benchmark
omarchy-shell shell call io.github.jwkicklighter.snipboard metrics ''
```

These hooks edit only the plugin's disposable TextEdit, using fixed synthetic fixtures. They do not produce physical keyboard events, read the system clipboard, copy content, or touch external applications. Scratch expansion starts off each time the panel opens and is turned off after a suite. Diagnostics contain timing arrays and booleans, not input text or payloads. See performance documentation for what the frame signal can and cannot measure.

Use `python -B` for live probes to avoid bytecode-file writes triggering the host's installation watcher. After a shell restart, wait for `omarchy-shell shell ping` and plugin initialization before starting a probe; IPC availability alone need not imply plugin readiness.

Manual workflow: open the overlay, choose **Scratch**, enable scratch expansion, then type `!email`, `!unicode`, `!multi`, or `!vars`. `{clipboard}` comes from the explicit field above it. **Insert selected into scratch** is distinct from the external-paste action. Search selection-copy stays ordinary; a dedicated **Copy only** button makes result copying explicit.

Snippet editing (after separately approved activation): choose **New**, or select a snippet and choose **Edit**. Save with the button or Ctrl+S/Super+S. Name, body/template, optional keyword, and notes stay in prototype `library.json`; nothing is imported from existing snippets. Escape/Cancel preserves a dirty draft until you explicitly discard it. Delete asks for confirmation. Validation/save failures retain the draft. **Reload library** explicitly adopts the current valid file, or retains the previous library read-only on failure; it is unavailable while the editor is open. External changes are detected at save/reload, not watched. This is local editing, not complete folder sync or conflict review.

## Disable and remove without deleting source or data

Disable unloads the overlay/helper and its owned clipboard process. It does not erase prototype data. The built-in clipboard picker needs no restoration because it was never replaced.

```sh
omarchy plugin disable io.github.jwkicklighter.snipboard
```

Uninstall the development placement by **moving the repository back** to Work (not deleting it), then refresh discovery:

```sh
cd "$HOME/.config/omarchy/plugins/io.github.jwkicklighter.snipboard"
python -m tools.development uninstall
cd "$HOME/Work/omarchy-snipboard"
omarchy-shell shell rescanPlugins
```

Alternatively, to remove **only** the Work shortcut while retaining the repository/installation:

```sh
cd "$HOME/.config/omarchy/plugins/io.github.jwkicklighter.snipboard"
python -m tools.development unlink-work
```

The latter removes only the verified symlink. Neither operation deletes the project or `${XDG_STATE_HOME:-$HOME/.local/state}/omarchy-snipboard-prototype`. **Do not use `omarchy plugin remove` for this placement:** that generic removal command is not the source-preserving uninstall procedure.
