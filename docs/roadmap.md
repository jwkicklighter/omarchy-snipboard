# Implementation roadmap

## Current scope

This repository currently contains planning documentation only. The feature goals are the product direction; the phases below are a proposed delivery sequence, not a release commitment.

## Phase 1: Validate the integration and performance foundation

- Confirm the supported Omarchy shell/plugin contract and Hyprland versions.
- Prototype a theme-aware panel, keyboard focus restoration, and copy/paste into representative Wayland apps.
- Independently prove safe typed-keyword detection and insertion, including capability detection, exclusions, focus changes, session lock, and multiline behavior.
- Benchmark a representative in-memory search dataset before choosing the final storage/search architecture.
- Select the permanent namespaced plugin ID and document dependencies and privilege boundaries.

**Exit:** The two highest-risk promises—low-latency search and safe cross-app expansion—have measured feasibility evidence and documented limitations.

## Phase 2: Ship the core picker and snippet library

- Build the compact search-first UI with full keyboard/mouse parity and live theme integration.
- Add persistent text snippets, editing, optional keywords, and validated import/export.
- Integrate clipboard capture, text/image previews, copy/paste, pause, deletion, pinning, and retention controls.
- Add persistent usage-aware snippet ordering with relevance-first matching and deterministic tests.
- Establish storage recovery, permissions, lifecycle cleanup, and regression tests.

**Exit:** The primary open → search → reuse workflow is reliable, theme-native, and within measured search budgets.

## Phase 3: Complete expansion and preview coverage

- Productize optional expansion with configurable keyword behavior and app exclusions.
- Test layouts, Unicode, backspace correction, overlapping keywords, injected-input suppression, focus loss, and locked sessions.
- Extend content classification and previews to links, files, structured text, colors, and safe rich text.
- Add optional local preview providers for document/media content, OCR, and QR decoding with graceful dependency fallbacks.
- Verify mixed-content history and large payloads do not regress search latency.

**Exit:** The full core feature goals have acceptance coverage; optional enhancements remain clearly identified.

## Phase 4: Release hardening

- Run an app/layout/theme compatibility matrix and publish supported versions and known limitations.
- Validate the plugin manifest, shell lifecycle, install/update/remove behavior, and restoration of any built-in behavior it replaces.
- Publish reproducible performance results and user-facing privacy/data-management documentation.
- Add installation instructions only after a runnable implementation exists.

## Open implementation decisions

- Extend/replace the built-in clipboard entry point, or provide an independent panel and capture service? Avoid competing capture owners and preserve existing history.
- Which shell theme APIs and plugin kinds provide the best native panel lifecycle?
- Which persistent data format and indexing strategy meet performance and recovery needs?
- What exact recency-decay function, frequency weighting, and tie-break rules best satisfy ranking tests?
- Should expansion fire immediately or on a delimiter by default? How should keyword prefixes, case, and boundaries behave?
- Which Hyprland input APIs are available, and what fallback is acceptable when safe expansion is unavailable? Clipboard/snippet browsing should remain usable.
- How can insertion preserve user input, clipboard contents, and multiline safety across supported applications?
- Which preview formats are built in versus optional, and what payload/cache limits protect the shell?
- What reference hardware and application matrix should define release performance and compatibility gates?

## Explicitly outside the initial scope

- Cloud synchronization, accounts, collaboration, or a hosted backend.
- Executing snippet bodies as scripts or commands.
- A general-purpose calculator, workflow runner, or automation platform.
- Guaranteed expansion in every app, password field, keyboard layout, or compositor.
- Mandatory OCR, media-processing, or network dependencies for basic search and reuse.

No implementation code has been imported. Any future reuse of third-party code must receive a separate license review and retain required notices and attribution.
