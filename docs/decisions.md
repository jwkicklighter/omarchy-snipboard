# Confirmed product decisions

Confirmed with the project owner on 2026-10-15. These choices refine the [feature goals](feature-goals.md); they do not indicate implemented functionality.

| Area | Decision |
| --- | --- |
| Integration | Replace the default Omarchy clipboard picker; restore it when disabled/removed. Preserve existing history where practical. |
| Search | Combined search with All / Clipboard / Snippets filters. Empty search separates recent history from frequent snippets. |
| Reuse actions | Enter or clicking a result pastes into the previous app. Ctrl+C, Super+C, or a dedicated button copies only. |
| Layout | Centered compact floating panel, search above results, preview beside the list, shortcut and bar-icon access. |
| Editor | Name, exact text/template body, optional keyword, searchable notes, and variables. |
| Variables | `{date}`, `{time}`, `{clipboard}`, and `{cursor}` in the initial release. Date/time use local YYYY-MM-DD and 24-hour HH:mm; unknown placeholders stay literal and clipboard substitution is nonrecursive. |
| Expansion | Immediate, case-sensitive, word-start matching; reject keyword-prefix ambiguity initially. |
| Expansion scope | Global opt-in with pause and app exclusions; exclude password managers and locked sessions. Do not claim universal password-field detection. |
| Multiline and undo | Safe multiline insertion required; automatic Backspace-to-restore-keyword undo deferred. |
| Terminal safety | Refuse multiline auto-insertion if a safe context cannot be verified; offer copy-only rather than risking command execution. |
| History defaults | 1,000 items, 30 days, 250 MB for unpinned history; configurable. Pins exempt from automatic eviction, with storage displayed separately. |
| Sync | User-managed folder containing snippets and portable settings; no built-in network sync. Usage ranking, history, exclusions, shortcuts, and machine paths stay local. |
| Conflict/unavailable storage | Preserve versions for explicit review; show the last valid in-memory library read-only when storage is unavailable, without silently creating a replacement. |
| First release | Text/image history, snippet editing, usage-aware search, theme integration, typed expansion, variables, and folder sync. Advanced previews, OCR, and QR decoding later. |
| Implementation | Native Omarchy shell UI, with a local helper only as needed. Choose helper language after feasibility/performance prototypes. |
| License | MIT. |

## Confirmed compatibility targets

- Ghostty and Foot, including Neovim inside each terminal.
- Chromium, including browser text fields and web-app composition.
- Discord, Telegram, and Obsidian.
- US Dvorak and US QWERTY, including live layout switching.

Compatibility means tested, documented behavior, not unrestricted insertion into every input context. Unsafe or unverifiable contexts require an explicit fallback.

## Inspection and privacy boundary

The owner approved read-only inspection of installed versions, desktop application names, and keyboard layout identifiers. Clipboard history, snippet contents, application documents, window titles, and general keystrokes were not accessed. No desktop configuration was changed.

See [compatibility.md](compatibility.md) for the inspected baseline and pending test matrix.
