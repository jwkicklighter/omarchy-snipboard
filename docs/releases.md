# Independent engine and plugin releases

Snipboard follows Omastorm's distribution model. Git installs follow upstream
HEAD; plugin tags are `vX.Y.Z`. Rust engines are separately published immutable
GitHub Release assets tagged `engine-X.Y.Z`. Source may be ahead of the plugin's
committed [`engine/release.pin.json`](../engine/release.pin.json).

## Automated two-release handoff

1. Submit **one feature PR** containing the Rust engine and all related QML,
   JavaScript and tests. Bump the package version in `Cargo.toml` and the matching
   `snipboard` entry in `Cargo.lock` to a new X.Y.Z. Preserve dependency versions.
   Leave the engine pin and plugin release versions alone. UI-only changes need
   no engine bump when they work with the existing pin.
2. After merge, **Engine release handoff** creates an annotated `engine-X.Y.Z`
   tag at that source commit and explicitly dispatches **Engine release** at the
   tag. The tagged workflow builds/tests on Ubuntu 24.04 x86_64, checks ELF/glibc
   requirements, smoke-tests the worker and attests the **raw executable**.
3. The workflow uploads a draft, independently verifies the attestation on the
   exact candidate bytes, then automatically publishes the engine. Assets are:
   - `snipboard-worker-linux-x86_64` (raw executable)
   - `worker.json` (version, target, binary checksum, source commit)
   - `SHA256SUMS`
   - `release.pin.json` (candidate pin)
4. **Adopt published engine** runs on current `master`, rejects superseded engine
   versions, verifies the published binary's provenance, and opens or updates
   `automation/engine-X.Y.Z`. That PR changes **only** `engine/release.pin.json`
   with a `fix: adopt engine-X.Y.Z` commit. Required CI installs the actual
   published executable and runs the worker protocol integration suite against
   it, alongside source, QML and packaging tests. The PR auto-merges by squash
   only after up-to-date required checks pass.
5. With source and pin aligned, Release Please opens/updates the plugin release
   PR and enables the same checked auto-merge. Its merge produces a `vX.Y.Z`
   draft. Full tagged CI must pass before the plugin archive is published.

**Release policy** permits combined engine/UI features but requires a newer
engine version whenever engine implementation/build/test paths change. Pin
adoption must be pin-only, match the current source version, and advance the
previous pin. Plugin release metadata cannot advance during a pending handoff.
The plugin-release workflow also gates release creation and final publication on
source/pin alignment. Candidate features are tested from source; published-worker
integration and release packaging run once the new pin is available.

Git installs following mutable `master` can briefly see new QML with the previous
pin between the feature and pin merges. Automatic plugin releases are held during
this window; this is not an atomic update mechanism for Git installations.

### One-time GitHub setup

- Keep repository auto-merge and squash merging enabled.
- In the active repository ruleset for `master`, enable **Require branches to be
  up to date before merging**, keep the bypass list empty (including admins),
  and require these exact checks:
  - `Release policy`
  - `Rust 1.89 / Linux x86_64`
  - `Rust stable / Linux x86_64`
- Classic branch protection is also supported when no status-check ruleset is
  active: require the same checks, up-to-date branches, and admin enforcement.
- Do not require manual PR approvals for the bot-only pin and release PRs if
  fully unattended merging is desired. Required CI checks still apply.
- Supply `RELEASE_PLEASE_TOKEN` with contents and pull-request write access and
  administration **read** access to inspect protection. A GitHub App token or
  suitably scoped PAT can be used. Bot PRs and merges must trigger subsequent
  Actions runs; the default `GITHUB_TOKEN` is not suitable for those writes.
  The existing secret is reused for pin PRs and Release Please.
- Engine tagging and explicit workflow dispatch use `GITHUB_TOKEN` with contents
  and Actions write access. Explicit `workflow_dispatch` avoids GitHub's normal
  suppression of follow-on events created by that token, and dispatching at the
  engine tag preserves the attestation's expected source ref.

Auto-merge automation refuses unprotected branches, missing required checks or
administrator bypass rather than risking an immediate merge before CI. This
code does not configure repository protection or add secrets automatically.
Automation branch names are bot-owned; do not use them for unrelated edits.

### Retries and manual recovery

Rerun **Engine release handoff** on `master` to resume the current version. Existing
tags are never moved; reuse requires identical engine source. Published engine
releases are reused without rebuilding or replacing their bytes. Failed draft
builds can be retried; reruns may replace draft assets only. A superseded engine
cannot open a pin PR for an older version. Never fix a published release in place;
increment the Cargo version instead.

To retry adoption of an already published current engine, dispatch **Adopt
published engine** with its `tag`. To retry the UI stage, run **Plugin release**
on aligned `master`. Preserve failed drafts for diagnosis; do not move tags.

For manual pin recovery, run this on a pin-only branch:

```sh
python3 tools/engine_release.py pin engine-X.Y.Z
```

Requires Python 3.11+, `readelf`, network access and a current `gh` supporting
`attestation verify` with `--signer-workflow`, `--source-digest`, `--source-ref`
and `--deny-self-hosted-runners`. It checks published/stable status, source tag,
checksums, architecture/glibc requirements, metadata, and signed SLSA provenance
for this repository's engine-release workflow, exact source commit and tag ref,
and a GitHub-hosted runner. Failure leaves the pin unchanged; there is no
checksum-only bypass. The pin command never commits, pushes, executes downloaded
code or publishes anything. CI deliberately executes verified workers against
disposable protocol fixtures; metadata equality alone is not compatibility proof.

### Reviewing provenance and migrating older releases

After downloading the raw release asset, reviewers can independently verify it
without executing it (replace the tag and source commit with the release's values):

```sh
gh attestation verify snipboard-worker-linux-x86_64 \
  --repo jwkicklighter/omarchy-snipboard \
  --signer-workflow jwkicklighter/omarchy-snipboard/.github/workflows/engine-release.yml \
  --source-digest FULL_ENGINE_SOURCE_COMMIT \
  --source-ref refs/tags/engine-X.Y.Z \
  --predicate-type https://slsa.dev/provenance/v1 \
  --deny-self-hosted-runners
```

An archive-only digest, `worker.json`, or a fresh checksum/attestation of an old
download does not establish source-to-binary correspondence. Older unattested
releases cannot be newly adopted by the pin command. Publish a new engine version
through the updated tagged build workflow, then adopt its attested pin in a
separate pin-only PR. Do not replace published assets or move old tags. Existing runtime
pins and offline caches continue to work; end users do not need `gh` or an online
attestation check at startup.

For marketplace review, submit the exact merged **plugin** commit containing the
new pin (not the engine commit), and link the engine release, build run and
attestation. If replacing an older Verify target, update the request for fresh
checks rather than treating evidence for a new binary as approval of the old one.

## First-run installation

QML runs `python3 tools/worker_runtime.py` before launching the worker or clipboard
capture. It downloads **only** the committed pin, checks SHA-256 and the ELF
architecture, and atomically installs it under:

```text
${XDG_DATA_HOME:-$HOME/.local/share}/omarchy-snipboard/workers/<sha256>/snipboard-worker
```

The checksum is rechecked on startup. A valid cached worker works offline; a
missing/corrupt one needs GitHub access. Installation is locked and staged on the
destination filesystem. A failed download/check never launches unverified code
or falls back to an older version. Pins coexist, so reverting a UI/pin PR can
reuse its cached engine. No Cargo, build, latest-version lookup, background
updater, or system package installation runs on users' machines. Checksums detect
corruption, not independent provenance or marketplace security approval.

Failures appear in the picker; diagnostics are in
`${XDG_DATA_HOME:-$HOME/.local/share}/omarchy-snipboard/bootstrap.log`. Fix the
network/storage/platform issue, then reload or restart the shell to retry.
History/snippets are stored separately and never modified by the installer.

## Publish the plugin

Release Please maintains a plugin release PR from Conventional Commits. Its
**simple** strategy updates `.plugin-version`, `manifest.json`, the release
manifest and changelog, **not Cargo metadata or the engine pin**. The automation
enables auto-merge only for this metadata-only release PR, behind required CI.

The `vX.Y.Z` draft triggers full CI at that tag, including installing/smoke-testing
its published worker pin. On success, automation uploads the plugin-only archive
and checksum and publishes the plugin release. Archives contain QML, metadata,
docs, pin and installer, never Rust binaries/private data. Failed checks leave
the draft unpublished. Retry failed jobs in the original run; do not move tags.

Configure the protection and token described above. Do not copy local credentials
into CI. Actions and the upstream validator are commit-pinned; review pins
periodically. Ordinary PR test jobs receive no release secrets/write permissions.

## Marketplace

Initial publication remains a separate owner-approved submission. Review
[marketplace-submission.md](marketplace-submission.md), confirm its checklist,
and follow the marketplace's
[submission instructions](https://github.com/omacom/omarchy-plugin-marketplace/blob/main/SUBMISSION.md).
Disclose the first-run download/dependencies. Maintainers decide approval;
automation cannot bypass review.

After initial approval, set `MARKETPLACE_SUBMITTED=true` and supply a dedicated
`MARKETPLACE_TOKEN` able to create marketplace issues. Plugin releases can then
open exact-commit update requests; engine releases do not. Retry using Actions →
**Retry marketplace update**, or preview without creating an issue:

```sh
python3 tools/marketplace.py --tag vX.Y.Z --dry-run
```

Requests require a published stable plugin tag at default-branch HEAD. Do not
advance HEAD during review; otherwise prepare a newer request. Existing requests,
including closed ones, are reused. Review covers an exact commit; normal plugin
updates follow mutable upstream HEAD.
