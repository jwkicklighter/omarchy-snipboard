#!/usr/bin/env python3
"""Request marketplace promotion; actual publication requires maintainer approval."""

import argparse
import json
import re
import subprocess
from urllib.parse import quote

REPOSITORY = "jwkicklighter/omarchy-snipboard"
MARKETPLACE = "omacom/omarchy-plugin-marketplace"
PLUGIN_ID = "io.github.jwkicklighter.snipboard"


def gh(*args, body=None):
    return subprocess.run(["gh", *args], input=body, text=True, check=True, capture_output=True).stdout.strip()


def verification_body(commit, tag):
    if not re.fullmatch(r"[0-9a-f]{40}", commit):
        raise ValueError("A full commit SHA is required")
    if not re.fullmatch(r"v\d+\.\d+\.\d+", tag):
        raise ValueError("Expected release tag vX.Y.Z")
    return f"""### Verification action

Verify and publish a newer upstream commit

### Plugin ID

{PLUGIN_ID}

### Repository URL

https://github.com/{REPOSITORY}

### Target commit

{commit}

### Verification acknowledgment

- [x] I understand that only the exact target commit can become a verified marketplace snapshot and that verification is not a security audit.

<!-- snipboard-release:{commit} -->

Release: https://github.com/{REPOSITORY}/releases/tag/{tag}
The repository includes the Linux x86_64 worker. Standard `omarchy plugin add`
with `--enable` needs no Rust toolchain, build step, or first-run binary download.
Runtime dependencies and platform requirements are documented in the README.
"""


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--tag", required=True)
    parser.add_argument("--dry-run", action="store_true")
    args = parser.parse_args()
    # Validate input before using it in API paths or an issue title.
    verification_body("0" * 40, args.tag)
    release = json.loads(gh("api", f"repos/{REPOSITORY}/releases/tags/{quote(args.tag, safe='')}"))
    if release["draft"] or release["prerelease"]:
        raise ValueError("Only published, non-prerelease releases can request promotion")
    commit = gh("api", f"repos/{REPOSITORY}/commits/{quote(args.tag, safe='')}", "--jq", ".sha")
    branch = gh("api", f"repos/{REPOSITORY}", "--jq", ".default_branch")
    head = gh("api", f"repos/{REPOSITORY}/commits/{quote(branch, safe='')}", "--jq", ".sha")
    if commit != head:
        raise ValueError("Marketplace requires current default-branch HEAD; this release is stale")
    body = verification_body(commit, args.tag)
    if args.dry_run:
        print(body)
        return
    # Search all pages, including closed requests, so retries cannot duplicate one.
    pages = json.loads(gh("api", "--paginate", "--slurp", f"repos/{MARKETPLACE}/issues?state=all&per_page=100"))
    marker = f"<!-- snipboard-release:{commit} -->"
    for page in pages:
        for issue in page:
            if marker in (issue.get("body") or ""):
                print(f"Request already exists (edit it to retry marketplace validation): {issue['html_url']}")
                return
    print(gh("issue", "create", "--repo", MARKETPLACE, "--title", f"[Verify]: Snipboard {args.tag}", "--body-file", "-", body=body))


if __name__ == "__main__":
    main()
