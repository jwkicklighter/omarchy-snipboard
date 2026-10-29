# Independent engine and plugin releases

Snipboard follows Omastorm's distribution model. Git installs follow upstream
HEAD; plugin tags are `vX.Y.Z`. Rust engines are separately published immutable
GitHub Release assets tagged `engine-X.Y.Z`. Source may be ahead of the plugin's
committed [`engine/release.pin.json`](../engine/release.pin.json).

## Ownership and PR order

1. Submit Rust source, Cargo/version, and Rust test changes in an **engine PR**.
   Do not change QML, the plugin manifest, or the worker pin in that PR.
2. Merge and publish the engine if UI work depends on it.
3. Submit compatible QML and a verified published pin together in a **UI PR**.
   UI must never require an unpublished engine. UI-only fixes can keep their pin.

CI's **Separate engine and UI PRs** check enforces this boundary, including
renames. Shared docs/release tooling are neutral; `tools/worker_runtime.py` is
plugin delivery code. Require this check and the Rust test checks in branch
protection. Breaking protocol changes can land in source while the existing UI
keeps its old pin. Do not update the pin just because source changed.

## Publish an engine

1. In an engine PR, update the package version in `Cargo.toml` and the matching
   `snipboard` entry in `Cargo.lock` to a new X.Y.Z. Preserve dependency versions.
   Do **not** bump `manifest.json`, `.plugin-version`, or the pin.
2. Run `make build test validate`. After merge, from clean `master` freshly
   matching `origin/master`, create and push only the annotated engine tag:

   ```sh
   git fetch origin
   git diff --exit-code origin/master
   git tag -a engine-X.Y.Z -m 'Engine X.Y.Z'
   git push origin refs/tags/engine-X.Y.Z
   ```

3. `.github/workflows/engine-release.yml` builds/tests on Ubuntu 24.04 x86_64,
   verifies the Cargo version/tag and ELF/glibc requirements, smoke-tests the
   worker handshake, and uploads these draft release assets:
   - `snipboard-worker-linux-x86_64` (raw executable)
   - `worker.json` (version, target, binary checksum, source commit)
   - `SHA256SUMS`
   - `release.pin.json` (candidate pin, not an automatic repository update)
4. Review the **draft**, then publish it in GitHub. Do not replace published
   assets or move tags; fix mistakes with a new version. Reruns can repair a
   draft but refuse a published release. Manual workflow runs build candidate
   artifacts only; they do not create tags or releases.
5. On a UI working branch, adopt the published release:

   ```sh
   python3 tools/engine_release.py pin engine-X.Y.Z
   ```

   Requires Python 3.11+, `gh`, and `readelf`. It checks published/stable status,
   resolves the tag's commit, downloads all four assets from the fixed repository,
   verifies checksums, architecture/glibc requirements and build metadata, then
   atomically updates the tracked pin. Failure leaves the pin unchanged. It never
   commits, pushes, executes downloaded code, or publishes anything.
6. Include the pin with compatible UI, run `make test package`, and review before
   merging. Packaging installs and smoke-tests the **published pin**, never a
   source-built substitute. CI also tests source-built candidates independently.
   Metadata equality alone is not feature compatibility coverage.

The first independent release (`engine-0.4.2`) is a one-time migration cut from
the existing local worker fixes plus an engine-only Cargo version commit on
`release/engine-0.4.2`. It is published explicitly because the new workflow is
not upstream yet. Merge that version branch through normal review; subsequent
releases use the workflow above. The plugin migration itself changes no Rust.

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
manifest and changelog, **not Cargo metadata or the engine pin**. Review and merge
normally; there is no binary commit job or automatic PR merge.

The `vX.Y.Z` draft triggers full CI at that tag, including installing/smoke-testing
its published worker pin. On success, automation uploads the plugin-only archive
and checksum and publishes the plugin release. Archives contain QML, metadata,
docs, pin and installer, never Rust binaries/private data. Failed checks leave
the draft unpublished. Retry failed jobs in the original run; do not move tags.

Configure `RELEASE_PLEASE_TOKEN` with repository contents and PR write access so
bot PRs/tags trigger Actions. Do not copy local credentials into CI. Actions and
the upstream validator are commit-pinned; review pins periodically. PRs receive
no release secrets/write permissions. Require CI and review on `master`.

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
