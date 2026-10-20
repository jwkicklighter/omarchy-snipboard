# Releases and Omarchy marketplace

## What is automated

- `.github/workflows/ci.yml`: pushes on every branch, pull requests, manual runs,
  and weekly checks using Rust 1.89 and the current stable toolchain. Runs the
  Rust, keyboard-layout, Qt Quick, Lua, worker, and Python packaging tests; checks
  the manifest using a pinned upstream Omarchy Quattro validator; packages and
  starts the worker from an extracted release bundle. CI never activates a real
  desktop plugin. Full Omarchy QML lint and live compositor tests remain local.
- `.github/workflows/release.yml`: `v*` tags run the same checks before publishing
  the Linux x86_64 bundle and checksum. The tag must match all three package
  versions. No release is created if tests, packaging, or version checks fail.
- An opt-in release job opens a marketplace **update request**, not an approval.
  `.github/workflows/marketplace.yml` can retry that step for an existing release.
  Existing requests for the same commit are reused, including closed ones; edit
  the existing issue to trigger marketplace validation again.

All third-party Actions and the upstream manifest validator are pinned to commit
SHAs. Review/update those pins periodically. Rust stable itself intentionally
tracks the latest stable toolchain. No untrusted pull request receives secrets
or release write permissions.

## Initial marketplace submission (one-time, owner approval required)

The repository is public, contains one root manifest with the permanent ID
`io.github.jwkicklighter.snipboard`, a root MIT license, and installation,
dependencies, privacy, update, and removal instructions in the root README.
The root `preview.png` is a full-screen capture using fictional sample data. The
marketplace discovers this filename automatically and generates optimized card
and detail images; no manifest field or manual image resizing is required. The manifest retains
the Prototype name/status rather than claiming unfinished features are stable.

1. Commit and push this configuration to `master`; ensure CI passes.
2. Search the marketplace for the permanent ID to confirm no conflict/retired ID.
3. Review the completed draft in
   [marketplace-submission.md](marketplace-submission.md). The owner must confirm
   **all five checklist statements**, including ownership, before submission.
4. With explicit owner approval, open the initial issue:

   ```sh
   gh issue create --repo omacom/omarchy-plugin-marketplace \
     --title '[Plugin]: Snipboard Prototype' \
     --body-file docs/marketplace-submission.md
   ```

5. Address compatibility/security-baseline feedback in the same issue. Ask for
   the marketplace's `manual-setup` classification: source installs require a
   worker build. There is intentionally no download-and-execute installer.
6. Wait for marketplace maintainer review and `approved-and-verified` publication.
   Only after the listing exists should update-request automation be enabled.

The draft is not an issued submission or a claim of owner approval. Do not open
duplicates. No submission, tag, release, secret, or remote setting is created by
merely adding these files.

## Enable automatic marketplace update requests

The normal repository `GITHUB_TOKEN` cannot open issues in the marketplace's
repository. Supply a **dedicated** GitHub credential able to create issues in
`omacom/omarchy-plugin-marketplace` as Actions secret `MARKETPLACE_TOKEN`.
The token's account must actually have that access; a fine-grained token scoped
only to this plugin repository will not work. A classic token with `public_repo`
can create issues in a public marketplace but grants broader access: prefer an
appropriately scoped GitHub App/fine-grained credential if the marketplace grants
one. Never commit a token or copy an existing local login credential into CI.

After configuring the secret and after initial listing approval:

```sh
gh variable set MARKETPLACE_SUBMITTED --body true
```

You can configure the secret interactively with
`gh secret set MARKETPLACE_TOKEN` or in GitHub Settings → Secrets and variables →
Actions. The workflow fails visibly if enabled without a working credential;
GitHub release publication remains successful and the marketplace job is
independently retryable. Disable update requests by setting the variable to
`false` or removing it.

## Publish a version

1. Update `manifest.json` and `Cargo.toml` to the same `X.Y.Z` and regenerate
   `Cargo.lock` (`cargo check`). Do not reuse a released version or move tags.
2. Run `make build test validate package` on Omarchy. Review runtime compatibility
   separately; successful offscreen CI does not prove all external apps work.
3. Commit with a Conventional Commit message and push `master`.
4. Tag the **current default-branch HEAD** and push:

   ```sh
   git tag -a v0.1.0 -m 'Snipboard v0.1.0'
   git push origin v0.1.0
   ```

   Substitute the new version. GitHub Actions publishes the tested artifact; no
   manually built binary is uploaded. Tag rules/protection may be enabled by the
   owner in GitHub Settings to restrict who can create release tags.
5. If opted in, the workflow opens `[Verify]: Snipboard vX.Y.Z` with the exact
   release SHA. It refuses stale tags, draft releases, and prereleases. Marketplace
   updates require current upstream HEAD, so avoid advancing `master` while the
   request is under review; otherwise prepare a newer release/request.
6. A marketplace maintainer reviews the exact commit and approves promotion.
   There is no supported API to bypass this step or automatically approve it.

Retry an update request with Actions → **Retry marketplace update**, entering the
published tag, or rerun the failed release job. For a read-only preview:

```sh
python3 tools/marketplace.py --tag v0.1.0 --dry-run
```

This contacts GitHub to check the release and HEAD, but does not create an issue.
GitHub CLI (`gh`) must be installed/authenticated for marketplace tooling.

## Upstream requirements

- [Submission instructions](https://github.com/omacom/omarchy-plugin-marketplace/blob/main/SUBMISSION.md)
- [Verification/update policy](https://github.com/omacom/omarchy-plugin-marketplace/blob/main/VERIFICATION.md)
- [Publishing guide](https://plugins.omarchy.org/publish.html)

Marketplace checks are not a security audit. Publication, verified snapshots,
GitHub releases, and mutable source installation are distinct operations.
