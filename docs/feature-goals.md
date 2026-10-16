# Feature goals

## Product intent

Make finding and reusing copied content or saved text feel immediate. A single Omarchy-native interface should serve both deliberate browsing and rapid keyboard-driven retrieval; optional typed keywords should make common snippets available without opening it.

This document specifies intended product behavior, not a claim that it is implemented. See [prototype status](prototype.md) and [actual measurements](performance.md). The performance numbers below remain acceptance targets.

## 1. Keyboard and mouse parity

- Replace Omarchy's default clipboard picker, restoring it when disabled or removed and preserving existing history where practical. Avoid competing clipboard capture owners.
- Open the picker through the existing clipboard shortcut, a configurable shortcut, or a clickable bar entry.
- Focus search immediately when opened; support selection, navigation, copy, paste, editing, and dismissal without a mouse.
- Provide discoverable mouse controls for the same actions, with tooltips and visible focus states.
- Search clipboard history and snippets together, with All, Clipboard, and Snippets filters. With an empty query, show recent history and frequently used snippets in distinct groups.
- Enter or clicking a result pastes into the previous app. Ctrl+C, Super+C, or a dedicated button copies only. Escape clears search before closing.
- Inside an editable field with selected text, standard copy shortcuts copy that selection rather than unexpectedly copying a result. Show which action applies.
- Return to the previously focused app when pasting. Closing without choosing must not alter its text or the clipboard.
- Keep destructive actions distinct from ordinary selection and provide confirmation or undo.

**Acceptance:** A user can open, find, preview, and reuse an item using only the keyboard, or only the mouse. Editing fields must retain normal cursor-navigation behavior rather than having it intercepted by view-switch shortcuts.

## 2. Polished, Omarchy-native presentation

- Use a compact centered floating panel with a prominent search field, readable rows, restrained borders, consistent spacing, and subtle action controls. Place the selected-item preview beside the list.
- Show a clear title and a shortened content summary in each row, with full content in a selected-item preview.
- Provide a focused snippet editor with clear save/cancel behavior and unsaved-change protection.
- Derive all UI colors from active Omarchy theme tokens, including surfaces, text, selection, borders, and semantic states. Do not impose a fixed brand palette.
- Follow shell typography, sizing, and interaction conventions; react to theme changes without a restart.
- Maintain readable contrast, obvious focus, and useful empty, loading, and error states in light and dark themes.
- Allow a compact list-only presentation without sacrificing access to previews.

**Acceptance:** Switching Omarchy themes updates every plugin surface, including editors and dialogs. The panel stays useful with long content and at different display scales.

## 3. Clipboard history and rich previews

- Capture supported clipboard content into persistent, searchable local history.
- Track content type, capture time, and source app where reliably available.
- Provide copy-only and paste actions that preserve the supported payload and MIME type; offer plain-text reuse separately where meaningful.
- Deduplicate repeated captures and avoid feedback loops from the plugin's own copy/paste operations.
- Support pinning, individual deletion, explicit history clearing, and configurable count, age, and storage limits. Snippets are independent of history eviction.
- Default to 1,000 unpinned items, 30 days, and 250 MB of unpinned history, evicting when any limit is reached. Pinned items are exempt from automatic eviction; display their storage separately so the cap is not mistaken for an overall disk bound.
- Provide a visible pause/resume recording control and honor sensitive-content hints where available. Do not claim these hints detect every secret.

### Preview coverage

| Content | Intended preview |
| --- | --- |
| Plain text, code, email addresses, numbers | Readable full text; useful type labels; code formatting where safe |
| Images | Scaled image, dimensions, and size |
| URLs | Address and local metadata; opening requires an explicit action |
| File lists | Names, paths, types, sizes where available, and missing-file state |
| JSON | Validated, formatted content with raw-content fallback |
| HTML or rich text | Safe inert preview or text fallback; no scripts or remote loads |
| Colors | Swatch and textual color value |
| PDFs, audio, video | Thumbnail or media metadata when an optional local preview provider is available |
| Unknown or unsupported data | MIME type, size, and a clear fallback rather than a broken preview |

Preview formatting must never silently change what is copied or pasted. OCR and QR decoding are optional enhancements, not prerequisites for capture or search. Large, malformed, unsupported, and missing content must fail gracefully. Previews are generated locally and must not execute copied content or fetch linked resources automatically.

**Acceptance:** Every captured item has a useful preview or an explicit fallback. Slow preview work never blocks navigation or search. Removing history also removes associated cached payloads when no retained item needs them.

## 4. Custom snippet library

- Create, edit, duplicate, delete, and search persistent text snippets.
- Store a stable ID, display name, exact text body, optional searchable notes, and an optional expansion keyword.
- Preserve Unicode, whitespace, tabs, and multiline text; notes and names are never inserted with the body.
- Keep snippets available indefinitely unless explicitly deleted, regardless of clipboard retention limits.
- Allow a clipboard text item to become a snippet without retyping it.
- Support versioned local import/export with validation and explicit conflict handling; invalid imports must not overwrite the current library.
- Include template variables in the first release: `{date}` (local YYYY-MM-DD), `{time}` (local 24-hour HH:mm), `{clipboard}` (a bounded snapshot of current clipboard text), and `{cursor}` (final caret position).
- Resolve variables at reuse time, once per action. Treat inserted clipboard text as literal, never recursively expand it, and leave unknown placeholders unchanged. A template's stored body is not rewritten by rendering.
- Support a user-selected sync folder for snippet definitions and portable settings, using the user's external synchronization tool rather than implementing a network service. Keep clipboard history, usage statistics, exclusions, shortcut bindings, and machine-specific paths local.
- Surface detected sync conflicts for explicit review without silently overwriting either version. If the folder is unavailable or invalid, retain the last valid in-memory library for reading and block edits until safe storage is available. Never silently replace it with an empty library.

**Acceptance:** A snippet with no expansion keyword remains fully usable through the picker. Stored content survives shell restarts and plugin updates byte-for-byte; insertion preserves literal content apart from explicit variable substitution. External sync changes reload safely, conflicts remain recoverable, and missing storage never causes writes to an unintended fallback location.

## 5. Usage-aware snippet ranking

- Persist successful snippet usage from picker copy/paste and typed expansion against the stable snippet ID.
- Combine frequency with recency decay: repeated recent use should beat equally relevant but stale matches, rather than letting lifetime totals dominate forever.
- Rank textual relevance before usage preference. Exact keyword/name matches outrank weaker fuzzy matches even when those weaker matches are popular.
- Search names, keywords, bodies, and notes; prioritize keyword/name matches over incidental body matches.
- Use deterministic tie-breaking and avoid selection jumps while an unchanged query is being navigated.
- Do not count previews, highlights, failed actions, or canceled actions as usage.
- Keep normal clipboard chronology distinct from learned snippet ordering and allow usage statistics to be reset without deleting snippets.

**Acceptance:** Controlled tests show that repeated recent use promotes equally relevant snippets, old usage loses influence, exact matches still win, and learning survives restart and renaming.

## 6. Optional typed-keyword expansion

- Let a snippet define a customizable keyword such as `!email`; keywords are optional and independent of display names.
- Detect a complete keyword typed in a supported app and immediately replace it with the rendered snippet body without opening the picker.
- Initially use case-sensitive, word-start matching. Reject duplicate keywords and keyword-prefix conflicts rather than delaying or guessing which immediate expansion is intended. Publish exact boundary rules before release.
- Expansion is globally opt-in. Provide a global enable/disable control, temporary pause, and per-app exclusions, including password managers by default.
- Handle Unicode, keyboard layouts, modifiers, backspace corrections, focus changes, and multiline content deliberately; show unsupported configurations explicitly.
- Reset partial keyword state when focus or input context changes. Never let injected replacement text recursively trigger another expansion.
- Do not expand while the session is locked, while the picker/editor owns input, or when the target app cannot be verified. Stop insertion if focus changes.
- Use only a bounded, short-lived matching buffer. Never persist or log general keystrokes, triggers, or snippet bodies in diagnostics.
- Do not promise password-field detection across all apps. Explain this limitation and provide conservative defaults and exclusions.
- Do not execute snippet text as shell commands. Multiline insertion must not submit chat messages or run terminal commands merely to insert a newline.
- Safe multiline insertion and `{cursor}` placement are required for the first release; automatic Backspace-to-restore-keyword undo is deferred.
- Refuse automatic multiline insertion at shell prompts when a safe insertion context cannot be verified; provide an explicit copy-only fallback. Do not assume terminal identity alone proves bracketed-paste support or the active application's input mode.

**Acceptance:** Typing `!email` replaces that keyword once, with the intended snippet, in documented supported apps. A keywordless snippet never expands. Paused, excluded, locked, or uncertain contexts produce no replacement and retain no partial matching state. Dvorak, QWERTY, and switching between them pass compatibility tests; unsupported insertion cases are explicit rather than unsafe best-effort behavior.

## 7. Search performance is a core feature

The critical path is opening the picker, typing, and seeing correct results. Rich previews and background capture must not compete with it.

- Keep searchable metadata/indexes available in memory and update incrementally.
- Avoid synchronous disk reads, external process launches, full payload decoding, and thumbnail generation per search keystroke.
- Virtualize long lists and load expensive content only when needed; bound caches and background work.
- Cancel or ignore stale searches and previews so fast typing never restores older results.
- Separate UI work from capture, persistence, enrichment, and expansion processing as needed to protect the shell's responsiveness.

### Proposed performance gates

Measure on a documented reference Omarchy machine with 10,000 clipboard records and 1,000 snippets, including large payloads outside the search index. Record machine, versions, dataset, and cold/warm state.

- Warm invocation to focused, usable picker: p95 at or below 100 ms.
- Search input event to visibly updated results: p95 at or below 16 ms and p99 at or below 50 ms during rapid typing.
- Large previews, background captures, and statistics writes do not introduce search stalls above these budgets.
- Benchmark typed expansion separately: report keyword-detection latency and full replacement time by body length, including multiline cases. Target p95 at or below 100 ms from completed keyword to insertion beginning; correctness must not be traded for speed.

**Acceptance:** Automated ranking tests and a repeatable interactive benchmark validate correctness and end-to-end latency, not just matching-function speed. Targets may be revised only with recorded evidence and an explicit tradeoff.

## 8. Local ownership and safe lifecycle

- Operate locally without accounts, cloud services, telemetry, or network access for core functionality.
- Folder synchronization, if configured, is performed by a separate user-managed tool. Explain that synced files contain readable snippet text and portable settings; Snipboard does not encrypt them or upload clipboard history.
- Keep user data separate from plugin source and use owner-only permissions, bounded reads, validation, and atomic writes.
- Treat clipboard and snippet content as untrusted data, not command syntax; keep private payloads out of process arguments and logs where possible.
- Preserve the last known-good data when files are invalid or unavailable; surface errors rather than silently replacing the library with empty data.
- Disabling/removing the plugin must stop its watchers, input hooks, and runtime bindings without leaving duplicate capture processes behind.
- Preserve user data across updates and removal; provide a separate, explicit way to erase it.

**Acceptance:** Restart, disable/re-enable, update, and removal tests leave no orphaned runtime integrations and cause no unintended data loss.
