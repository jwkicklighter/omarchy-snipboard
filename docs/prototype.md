# Prototype boundary and safety design

## Implemented

- Native kept-loaded overlay hosted by the existing Omarchy Quickshell instance, using the overlay manifest contract (`open`, `close`, `opened`). Compact centered surface, focused editable search, virtualized results, side preview, keyboard/mouse controls, All/Clipboard/Snippets filtering.
- Live bindings to installed `Color.menu`, `Color.muted`, `Style` typography, spacing, focus, selection, and border tokens. Theme switching itself remains an interactive test, not a claimed pass.
- 10,000 generated clipboard records and 1,000 generated stable-ID snippets. Full payload strings are separate from bounded search metadata; each field indexes its first 512 case-folded Unicode characters. Text beyond that prefix is **not searchable** in this prototype. At most 60 results are returned. Empty All search reserves 30 places per group.
- C++/QtCore fuzzy matching inside the Python helper process, with a Python specification checked for equivalent ranking. Exact/prefix/substring/subsequence tiers, keyword/name priority over incidental body/notes, frequency with 14-day exponential recency decay, stable ID tie-breaks. Relevance always precedes usage. Clipboard chronology stays distinct.
- One in-flight search plus a coalesced latest query; stale responses ignored. No disk access, helper spawn, or payload decoding on the matching path. Previews are delayed 120 ms and skip unchanged selections; result delegates are retained where possible.
- Owner-only, validated, bounded JSON storage with same-directory temporary files, fsync, atomic replacement, duplicate-key rejection, stable IDs, and a single-worker lock. Invalid data is not silently reset. Reload retains the last valid library read-only; a cold invalid store fails closed. There is no external-change watcher or editing UI yet.
- Persistent usage after acknowledged scratch insertion; renaming does not change identity. Copy requests are **not counted**, because wl-copy does not provide an ownership-success acknowledgement here. Failed/canceled/preview actions do not increment usage.
- Opt-in disposable TextEdit expansion for `!email`, `!unicode`, `!multi`, `!vars`; plain-text insertion cannot send messages or execute commands. Unicode cursor offsets convert to Qt UTF-16 units.

## Actions and limitations

Enter or result click requests external paste, but all external adapters are currently unverified: the action **refuses**, explains why, and offers **Copy only** or the explicit scratch target. It does not silently inject keys or copy data on refusal. Copy-only writes rendered text via stdin to a managed `wl-copy --foreground`; payloads are never shell syntax or process arguments. Copy ownership ends on disable/unload, so content may become unavailable then. External copy behavior has not been interactively verified.

Ctrl+C/Super+C in search copies a selection if present, otherwise requests result copying. Other editable fields retain their normal selection-copy handlers; Super+C outside search still needs physical-layout validation. Escape clears search before dismissing when search owns focus; elsewhere it dismisses the panel. No global keybindings are registered.

## Template format

- `{date}` local YYYY-MM-DD; `{time}` local 24-hour HH:mm; both use one timestamp per action.
- `{clipboard}` is a bounded **explicit scratch input** (16,384 characters), not the real clipboard. It is substituted literally and nonrecursively.
- `{cursor}` marks a single caret position; repeated markers reject the action. Unknown placeholders stay literal. `{{date}}`, `{{time}}`, `{{clipboard}}`, `{{cursor}}` escape recognized tokens.
- Body/render limit: 65,536 characters. Stable IDs, names, notes, and keywords are validated separately; names/notes never become inserted text.

## Expansion feasibility and privilege boundary

Read-only inspection of installed Hyprland 0.56.2 found:

| API | Finding / decision |
| --- | --- |
| `/usr/share/hypr/stubs/hl.meta.lua` | `hl.on("input.keyboard.key", callback)`, removable `HL.EventSubscription`, removable keybinds. No hook installed. |
| `/usr/include/hyprland/src/event/EventBus.hpp` | Native cancellable keyboard-key events plus keyboard layout/focus events. A compositor C++ plugin has an unstable ABI and would run inside the compositor; not loaded. |
| Hyprland IPC / Quickshell Hyprland module | Context/layout event names can invalidate scratch work. Event payloads, including window titles, are not inspected. IPC is not a verified general text-input/atomic-insertion API. |
| Wayland input-method / virtual keyboard and installed wtype | Injection does not prove target editor mode, password-field safety, safe newlines, or focus-atomic replacement. Not used. No evdev permissions or capture portals requested. |

**Global expansion is blocked, not implemented.** Consequently every external app—including password managers, terminal prompts/editors, chat composers, unknown/locked sessions, and ordinary picker input—is denied by construction. There is no general keystroke listener, private buffer, or payload diagnostic log. The matcher model has a maximum 64-character ephemeral candidate and context/exclusion/injection gates, but model tests do not prove a production input adapter.

Keywords are optional, case-sensitive, immediate, and word-start-only. Duplicate and prefix-conflicting keywords reject validation. A word character is a Unicode letter/number/combining mark or underscore; punctuation/space or the known beginning of the scratch field is a boundary. Unknown external context is never presumed a boundary. Focus, cursor movement, layout/context events, and pause invalidate scratch requests; only newly typed triggers after a reset are eligible. Injected TextEdit changes are guarded and cannot recursively expand. Pending responses require the same revision, caret, opt-in state, and scratch focus before modifying the field.

Safety for an external adapter remains a prerequisite: verified focus identity, lock state, exclusions, exact deletion semantics, layout-aware input, reliable input-context changes, injected-event suppression, and verified newline/caret insertion. A terminal class alone proves none of those. Until then **all external multiline/cursor operations refuse**, with copy-only fallback. Automatic undo is out of scope.

## Data and eventual sync boundary

Prototype-only root: `${XDG_STATE_HOME:-$HOME/.local/state}/omarchy-snipboard-prototype/`.

| File | Boundary |
| --- | --- |
| `library.json` | Version 1 portable snippet definitions + narrowly allowlisted portable settings. No usage, history, exclusions, shortcuts, or machine paths. |
| `usage.local.json` | Local counts and last-used timestamps keyed by stable ID. |
| `initialized`, `worker.lock` | Local lifecycle markers; never portable. |

No existing history/library is imported, and no clipboard recording is started. Generated history exists only in memory. Data files are limited to 8 MiB, regular owner-only files; symlink data files/root are rejected. Atomic writes preserve the original on pre-replacement failure. An observed external library change blocks saving pending explicit review/reload, but full cross-machine concurrency/conflict handling is **not implemented**. This is a boundary design, not usable folder sync. Source updates/uninstallation preserve the data root.

## Cleanup

The QML-owned Process is stopped on destruction. Kernel parent-death SIGTERM covers shell exit; EOF/SIGTERM reap the helper's exact managed child. Its clipboard owner also has a parent-death signal. Native index memory is released. No watcher, runtime binding, or compositor hook is created by Snipboard; the host owns its source watcher. Mock and subprocess tests cover ordinary shutdown; actual disable/re-enable is a separate integration check. SIGKILL race windows and all compositor/shell failure modes are not exhaustively verified.

Actual integration check: one helper before disable, zero after disable, one new helper after re-enable, zero after final disable. No real clipboard-copy process was started in interactive testing, so its termination path is unit/mock-tested rather than an interactive clipboard-ownership pass. The existing shell remained responsive after testing.

## Not implemented

Real history capture/migration/retention/pinning, image/rich previews, snippet CRUD UI, import/export UI, folder sync/conflict UI, external expansion and paste adapters, automatic undo, OCR, QR decoding, bar entry, and replacement/restoration routing. No external application compatibility claim is made. Implementation is original; no third-party implementation source was imported.
