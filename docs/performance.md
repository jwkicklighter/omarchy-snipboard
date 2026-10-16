# Prototype performance measurements

Measured 2026-10-15 on this machine: Intel Core i5-5257U (2 cores / 4 threads), Linux 7.2.5-3-omarchy x86_64, Python 3.14.7, Qt 6.11.2, Quickshell 0.3.1, Omarchy 4.0.4, Hyprland 0.56.2. Shared desktop load and CPU scheduling are uncontrolled. No power/governor settings were changed.

Dataset: 10,000 synthetic clipboard records, 1,000 snippets, 1,137,938 body characters, including ten ~48k-character history payloads outside the bounded metadata index. Each indexed field is limited to 512 case-folded characters. Queries include empty, incremental `e/em/ema/email`, subsequences, common body terms, numeric, Greek, and no matches. Percentiles use nearest rank. All numbers below are milliseconds.

## Helper selection experiment

| Measurement | Samples | p50 | p95 | p99 |
| --- | ---: | ---: | ---: | ---: |
| Initial Python-only matching | 130 | 24.522 | 51.720 | 58.672 |
| Initial Python worker round trip | 130 | 26.028 | 54.742 | 57.345 |
| C++ matching through Python binding | 390 | 3.644 | 6.504 | 7.872 |
| Matching reported by persistent C++/Python worker | 390 | 3.744 | 7.189 | 11.019 |
| Sequential helper round trip | 390 | 4.161 | 8.328 | 12.031 |

C++ was selected **after measurement**, not assumed. Python remains appropriate for bounded persistence/template validation; it does not perform the hot matching loop. Cold native worker readiness was 196.505 ms (one sample), excluding shell/UI startup; this is not warm opening latency. No process is launched per query.

### Final source rerun

After Unicode normalization was moved consistently to the boundary and the UI probes/cleanup completed, a second 390-sample run gave:

| Final implementation | p50 | p95 | p99 |
| --- | ---: | ---: | ---: |
| Matching through binding | 6.318 | 11.829 | 27.805 |
| Worker-reported matching | 6.216 | 10.249 | 13.340 |
| Sequential helper round trip | 7.240 | 11.480 | 14.461 |

Cold readiness was 441.017 ms (one sample). Report both runs rather than selecting only the faster result: shared load/scheduling are uncontrolled, and neither matching-only run predicts UI responsiveness. Final raw synthetic diagnostics: `/tmp/opencode/snipboard-helper-final.json`.

### Rust rewrite

On 2026-10-16 the helper was rewritten as a single Rust binary: the Python worker, the Python reference matcher, and the C++/QtCore library were removed. Before removal, 1,251 searches (All/Clipboard/Snippets × 417 fixed and random queries) returned identical ranked IDs from the Rust worker and the Python specification, and library revisions matched byte-for-byte. One 390-sample run, same machine and dataset, Rust 1.98.1:

| Rust implementation | p50 | p95 | p99 |
| --- | ---: | ---: | ---: |
| Matching only (in-process) | 2.601 | 5.522 | 5.825 |
| Worker-reported matching | 4.239 | 5.833 | 6.015 |
| Sequential helper round trip | 4.483 | 6.247 | 6.509 |

Cold readiness was 79.502 ms (one sample). This is a single run under the same uncontrolled load as above; it is a helper measurement only and says nothing new about UI latency.

Reproduce the matching and round-trip measurements:

```sh
make benchmark
# equivalently, after `make build`:
build/release/snipboard-tools benchmark --rounds 30
```

These sequential tests include no rapid-query queueing, painting, input dispatch, or window activation. **They do not establish either end-to-end performance target.**

## In-shell UI timing: initial run

Approved test inside the existing shell, changing only the prototype's search field every 90 ms for 260 synthetic queries. No OS keyboard input was injected. `metrics` measures search-text change → model assignment and search-text change → QQuickWindow `frameSwapped`. It also measures `open()` receipt → focused/results-ready frame swap. Timing arrays contain no queries or payloads.

| Initial array-model implementation | Recorded samples | p50 | p95 | p99 |
| --- | ---: | ---: | ---: | ---: |
| Search-text change → model assignment | 218 | 21 | 71 | 130 |
| Search-text change → frameSwapped proxy | 211 | 46 | 81 | 112 |
| `open()` → focused frameSwapped proxy | 20 | 319 | 542 | 734 |

**Targets missed.** These are not matching-only measurements, but also not externally observed display or physical-keyboard latency. Coalescing skips stale queries; frame samples can be missing, so the sample count is reported rather than implying all 260 requests displayed. Timer scheduling, shared shell workload, font/layout cost, preview processing, and repeated JS-array model replacement all contribute. The in-loop hide/open is not an independent cold or real shortcut invocation test, and excludes IPC/shortcut delivery time.

The source now retains result delegates and delays previews by 120 ms. A repeat run must be identified separately; never substitute the matching numbers for UI results. The acceptance targets remain warm opening p95 ≤100 ms and visible search p95 ≤16 ms / p99 ≤50 ms. No passing claim is made until actual input-to-visible validation supports it.

### Final UI rerun (retained model, delayed preview)

| Final implementation | Recorded samples | p50 | p95 | p99 |
| --- | ---: | ---: | ---: | ---: |
| Search-text change → model assignment | 218 | 17 | 46 | 76 |
| Search-text change → frameSwapped proxy | 192 | 43 | 71 | 89 |
| In-loop open receipt → focused frame proxy | 20 | 314 | 380 | 384 |
| Separate IPC hide/wait/summon cycles → focused frame proxy | 20 | 193 | 244 | 262 |

All 20 separate warm-open samples reported focused search. This second open test actually hid the panel for 250 ms between summons, then allowed 750 ms before reading diagnostics. The measured interval still starts at `open()` receipt, **not before launching IPC**. The result therefore cannot establish full shortcut-to-visible performance, and even this narrower interval misses the target.

The frame signal is the first observed swap after model assignment, without a renderer-side frame tag or external display observation. Queued swaps, coalescing, and sampling loss limit it; it is explicitly a proxy. Hardware input handling is excluded. Raw synthetic arrays are `/tmp/opencode/snipboard-ui-final.json`, separate warm cycles `/tmp/opencode/snipboard-warm-final.json`. **UI targets remain missed/unproven; Phase 1 is not complete.** No matching-only result is labeled end-to-end.

Reproduce only after approving activation and scratch testing:

```sh
build/release/snipboard-tools probe benchmark
```

Do not build, edit documentation, or run tests concurrently: the installed host watches all installation files and may reload plugins. QML may remain cached after rescan; a separately approved shell restart may be necessary. Probe output lives under `/tmp/opencode`, not tracked data.

## Expansion measurements and gaps

The four in-shell TextEdit suites passed (email, Unicode, multiline, variables with literal clipboard substitution and cursor). This proves the local TextEdit insertion path, not cross-application expansion. The pure matcher, template renderer, Unicode caret conversion, keyword conflicts, gate resets, and both libxkbcommon layouts are automated-test covered.

The suite passed again with Dvorak active and with QWERTY active. A real compositor layout switch after a primed partial scratch trigger emitted an invalidation event and refused the stale completion. Dvorak was restored (two layout-reset events observed). This tests event/reset integration and simulated TextEdit edits, not physical key delivery. Synthetic diagnostics: `/tmp/opencode/snipboard-scratch-final.json` and `/tmp/opencode/snipboard-layout-final.json`.

No global keyword completion → external insertion timing exists because there is no safe external adapter. Physical-keyboard end-to-end latency, layout switching during physical typing, theme changes, real warm shortcut invocation, mixed capture workload, and external-app performance remain unverified. General key/payload logging was not used to obtain measurements.
