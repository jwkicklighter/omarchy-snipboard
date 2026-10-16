# Development baseline and compatibility matrix

## Picker paste update (2026-10-16)

The owner reported the original Shift+Insert picker paste worked in Omawrite but failed in Ghostty. Inspection of Ghostty's effective keybindings confirmed Shift+Insert reads the primary selection, while Ctrl+Shift+V reads the regular clipboard. Snipboard now uses Hyprland's explicit compositor key-state API, following Omarchy's universal clipboard bindings, with Ctrl+V for normal apps and Ctrl+Shift+V for Omarchy-tagged terminals. A separate 50 ms key release follows the key press, avoiding the virtual-keyboard modifier merging problem documented in Omarchy's binding source.

The rebuilt release worker passed a live disposable Ghostty test: a raw terminal reader with bracketed paste enabled received the exact fixed Unicode/multiline fixture. The test used a separate prototype library under `/tmp`, opened and closed only its own Ghostty window, and did not execute the pasted text. The clipboard was intentionally replaced with the synthetic fixture. Automated tests cover script routing, key press/release, template rendering, command failures, zero-status IPC errors, and overlay focus handoff. Manifest/QML validation passes with the known metadata warnings.

This does not establish compatibility with terminal shell prompts, Neovim modes, every app/layout, or external cursor positioning. Global keyword detection is still unimplemented; only Snipboard's scratch field provides keyword expansion. The earlier matrix and validation notes below are historical baselines.

## Inspected baseline

Read-only inventory performed on 2026-10-15. This is a development baseline, not a tested minimum-version claim.

| Component | Installed version |
| --- | --- |
| Omarchy | 4.0.4-1 |
| Hyprland | 0.56.2-2 (runtime reports 0.56.2) |
| Quickshell | 0.3.1-1 |
| Qt base | 6.11.2-3 |
| wl-clipboard | 2.3.0-1 |
| wtype | 0.4-2 |
| libxkbcommon | 1.13.2-1 |
| Rust (rustc/cargo) | 1.98.1 |
| Lua | 5.5.1-1 |

`qs`, `hyprctl`, `wl-copy`, `wl-paste`, `wtype`, and `xkbcli` are available on PATH. QML lint is installed at `/usr/lib/qt6/bin/qmllint`, not on PATH. Node.js and Syncthing were not found in the package/PATH checks; neither is currently a selected runtime dependency. A sync folder may be managed by any compatible external tool.

The installed shell documentation confirms that plugins run inside the shared Quickshell process. The built-in clipboard manifest is a kept-loaded overlay. Built-in clone routing provides a potential replacement/restoration path; the exact manifest and lifecycle behavior still require validation for this plugin. No second shell instance or system integration has been started.

## Required application matrix

The owner confirmed all entries below. Every test is **pending**; installed software is not evidence of compatibility.

| Target | Inventory | Required checks |
| --- | --- | --- |
| Ghostty | 1.3.1-2 | Copy/paste, shell prompt and Neovim, immediate expansion, multiline safety/fallback, cursor positioning |
| Foot | 1.28.0-1 | Copy/paste, shell prompt and Neovim, immediate expansion, multiline safety/fallback, cursor positioning |
| Neovim | 0.12.5-1 | Insert/normal-mode safety in both terminals, Unicode, multiline text, cursor positioning |
| Chromium | 152.0.7977.82-1 | Plain text fields, textareas, contenteditable, web-chat multiline safety, cursor positioning |
| Discord | Desktop launcher present; version/build not yet established | Message composer, multiline insertion without send, cursor positioning |
| Telegram | 7.2.5-1 | Message composer, multiline insertion without send, cursor positioning |
| Obsidian | Desktop launcher present; version/build not yet established | Markdown editor input, multiline insertion, cursor positioning |

Application packaging must be checked before writing exclusions or app-specific adapters: a desktop launcher name alone does not prove a native app versus a browser-hosted app. Do not assume terminal identity reveals whether a shell prompt or editor is active.

1Password is present in the desktop inventory and should be covered by default password-manager exclusion tests, not treated as an expansion target.

## Required keyboard matrix

- Active at inspection: English (Dvorak).
- Configured XKB layouts: `us,us`; variants: `dvorak,` (Dvorak and unmodified US QWERTY).
- Test expansion, search input, copy shortcuts, modifiers, shifted punctuation, and backspace correction under both layouts.
- Test layout switching with a partially typed keyword: stale matching state must not survive a context change.
- Confirm that shortcut behavior follows Omarchy conventions under Dvorak rather than assuming physical QWERTY key positions.

## Safety and lifecycle test gates

- Session lock/unlock, uncertain lock state, focus changes during insertion, excluded apps, picker/editor focus, and global expansion pause.
- No recursive expansion from injected text; no logging or persistence of matching buffers.
- Multiline content never sends chat messages or executes shell commands. If this cannot be guaranteed for a context, refuse automatic insertion and offer copy-only.
- `{cursor}` placement tested with Unicode, multiline content, and variable substitutions. Do not claim compatibility from ASCII-only tests.
- External sync updates, conflict files, malformed data, missing folders, and permission failures preserve recoverable data.
- Disable/re-enable, shell restart, plugin update/removal, and clipboard restoration do not leave hooks or capture processes behind.

Interactive validation must use scratch documents/input contexts with explicit approval before controlling apps, modifying desktop integration, or injecting text. The inventory itself did none of these.

## Prototype validation update (2026-10-15)

- Owner approved the source relocation/development link, enabling and opening this overlay (not replacing the clipboard picker), disposable scratch testing, temporary layout testing/restoration, and two separately approved shell restarts to defeat an observed component-cache issue.
- Native QML overlay loaded in the **existing** shell, and its search reported focused/results-ready. Internal TextEdit probes passed email, Unicode, multiline, literal clipboard variables, and cursor placement. They simulate field edits, not physical keys.
- Offline tests use the installed **libxkbcommon** to translate the actual US QWERTY and US Dvorak keymaps and complete `!email`. Matcher layout/context reset is unit-tested. This does **not** establish physical-keyboard or live-layout-switch compatibility.
- Final TextEdit suites passed with each compositor layout active. A real Dvorak→QWERTY switch invalidated a partial scratch keyword, preventing stale expansion; Dvorak was restored. Text edits were simulated through the prototype, not physical key presses, so physical layout compatibility is still unverified.
- All external application rows above remain **pending/unverified**. No commands or messages were inserted into terminals/chats, no private clipboard/history/library was read, and no general keyboard hook was installed.
- Theme bindings are present; actual theme-switch/light/dark/scale validation remains pending. External copy-only behavior is also pending; a copy request is not treated as an acknowledged successful use.
- Manifest validation passes. Installed QML lint exits 0 with reported host metadata warnings; see [development](development.md). [Performance](performance.md) separates matching, helper round trips, and render-loop proxies; initial UI targets were missed.
- All 23 automated tests pass. Actual disable/re-enable showed one helper when enabled and zero when disabled. The final state is **disabled**, with no global hooks/bindings/watchers added. Final UI timing targets still miss; the latest measurements and limitations are recorded separately from the initial run.
