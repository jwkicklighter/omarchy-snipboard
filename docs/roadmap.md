# Implementation roadmap

## Current scope

This repository currently contains planning documentation only. The feature goals are the product direction; the phases below are a proposed delivery sequence, not a release commitment.

The [confirmed decisions](decisions.md) define the first release: core clipboard/snippet functionality, usage-aware search, variables, user-managed folder sync, and optional typed expansion. Advanced preview providers, OCR, QR decoding, and automatic expansion undo follow later.

## Phase 1: Validate the integration and performance foundation

- Confirm the supported Omarchy shell/plugin contract and Hyprland versions.
- Prototype a theme-aware panel, keyboard focus restoration, and copy/paste into representative Wayland apps.
- Independently prove safe typed-keyword detection and insertion, including capability detection, exclusions, focus changes, session lock, and multiline behavior.
- Prove cursor placement and safe insertion in the confirmed app/layout matrix; explicitly identify contexts that must use copy-only fallback.
- Define the shared-versus-local settings schema, variable escaping rules, and recoverable sync-conflict behavior before choosing the storage format.
- Benchmark a representative in-memory search dataset before choosing the final storage/search architecture.
- Select the permanent namespaced plugin ID and document dependencies and privilege boundaries.

**Exit:** The two highest-risk promises—low-latency search and safe cross-app expansion—have measured feasibility evidence and documented limitations.

## Phase 2: Ship the core picker and snippet library

- Build the compact search-first UI with full keyboard/mouse parity and live theme integration.
- Add persistent text snippets, editing, optional keywords, date/time/clipboard/cursor variables, and validated import/export.
- Add user-managed folder sync for snippets and portable settings, external-change reload, conflict review, and missing-storage protection. Keep usage statistics and machine-specific settings local.
- Integrate clipboard capture, text/image previews, copy/paste, pause, deletion, pinning, and retention controls.
- Add persistent usage-aware snippet ordering with relevance-first matching and deterministic tests.
- Establish storage recovery, permissions, lifecycle cleanup, and regression tests.

**Exit:** The primary open → search → reuse workflow is reliable, theme-native, and within measured search budgets. This is an internal milestone, not yet the agreed first release without typed expansion.

## Phase 3: Complete the first-release core

- Productize globally opt-in, immediate, case-sensitive word-start expansion with prefix-conflict validation and app exclusions.
- Test layouts, Unicode, backspace correction, overlapping keywords, injected-input suppression, focus loss, and locked sessions.
- Complete safe multiline insertion and cursor placement, refusing unverified contexts with an explicit copy-only fallback.
- Cover the required apps and both layouts in the [compatibility matrix](compatibility.md).
- Verify mixed-content history and large payloads do not regress search latency.

**Exit:** The agreed core feature set has acceptance coverage, including sync, variables, and typed expansion. Advanced preview features remain explicitly deferred.

## Phase 4: Release hardening

- Run an app/layout/theme compatibility matrix and publish supported versions and known limitations.
- Validate the plugin manifest, shell lifecycle, install/update/remove behavior, and restoration of any built-in behavior it replaces.
- Publish reproducible performance results and user-facing privacy/data-management documentation.
- Add installation instructions only after a runnable implementation exists.

## Phase 5: Broader previews and enhancements

- Extend content classification and previews to links, files, structured text, colors, and safe rich text.
- Add optional local preview providers for document/media content, OCR, and QR decoding with graceful dependency fallbacks.
- Evaluate automatic expansion undo after the core insertion path is proven reliable.
- Continue measuring mixed-content search and bounding caches and background work.

## Open implementation decisions

- How should the replacement picker adopt existing clipboard history and ensure there is only one capture owner?
- Which shell theme APIs and plugin kinds provide the best native panel lifecycle?
- Which persistent data format and indexing strategy meet performance and recovery needs?
- What exact recency-decay function, frequency weighting, and tie-break rules best satisfy ranking tests?
- What precise word-boundary rules are safe after cursor movement, focus changes, or incomplete input context?
- What variable escape syntax, clipboard-text size bound, and rules for repeated cursor markers should the format use?
- Which settings are portable, and how should known sync conflict-file formats be detected without claiming universal conflict detection?
- Which Hyprland input APIs are available, and what fallback is acceptable when safe expansion is unavailable? Clipboard/snippet browsing should remain usable.
- How can insertion preserve user input, clipboard contents, and multiline safety across supported applications?
- Which preview formats are built in versus optional, and what payload/cache limits protect the shell?
- What reference hardware and application matrix should define release performance and compatibility gates?

## Explicitly outside the initial scope

- Built-in network synchronization, accounts, collaboration, or a hosted backend. User-managed folder sync for snippets and portable settings is in scope.
- Executing snippet bodies as scripts or commands.
- A general-purpose calculator, workflow runner, or automation platform.
- Guaranteed expansion in every app, password field, keyboard layout, or compositor.
- Mandatory OCR, media-processing, or network dependencies for basic search and reuse.

No implementation code has been imported. Any future reuse of third-party code must receive a separate license review and retain required notices and attribution.
