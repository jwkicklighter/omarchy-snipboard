#!/usr/bin/env python3
"""Coordinate immutable engine releases, pin-only PRs and checked auto-merges."""

import argparse
import json
import os
import re
import subprocess

from check_pr_split import groups
from engine_release import engine_version, gh, pin_release
from release_policy import ready, version
from worker_runtime import REPOSITORY, ROOT, read_pin

REQUIRED_CHECKS = {"Release policy", "Rust 1.89 / Linux x86_64", "Rust stable / Linux x86_64"}


def api(path, **fields):
    args = ["api", f"repos/{REPOSITORY}" + (f"/{path}" if path else "")]
    if fields:
        args += ["--method", "POST"]
        for key, value in fields.items():
            args += ["-f", f"{key}={value}"]
    return json.loads(gh(*args))


def output(name, value):
    value = str(value).lower() if isinstance(value, bool) else str(value)
    print(f"{name}={value}")
    if os.environ.get("GITHUB_OUTPUT"):
        with open(os.environ["GITHUB_OUTPUT"], "a") as destination:
            destination.write(f"{name}={value}\n")


def release(tag):
    # A list query distinguishes a missing release from authentication/API errors.
    pages = json.loads(gh("api", "--paginate", "--slurp", f"repos/{REPOSITORY}/releases?per_page=100"))
    return next((item for page in pages for item in page if item["tag_name"] == tag), None)


def published(item):
    if item and item["prerelease"]:
        raise ValueError("Engine releases must be stable")
    return bool(item and not item["draft"])


def check_tag(tag):
    if not re.fullmatch(r"engine-\d+\.\d+\.\d+", tag):
        raise ValueError("Expected engine-X.Y.Z")


def source_matches(commit):
    paths = subprocess.check_output(["git", "diff", "--name-only", commit, "HEAD"], text=True).splitlines()
    if groups(paths)[0]:
        raise ValueError("Engine source changed after this tag; bump the engine version instead of reusing it")


def dispatch(workflow, ref, tag=None):
    args = ["workflow", "run", workflow, "--repo", REPOSITORY, "--ref", ref]
    if tag:
        args += ["-f", f"tag={tag}"]
    gh(*args)


def route():
    if ready():
        print("Source and pin are aligned; no engine handoff needed")
        return
    source_version = engine_version()
    if version(source_version) <= version(read_pin(ROOT / "engine/release.pin.json")["version"]):
        raise ValueError("Pending engine version must be newer than the pin")
    branch = api("")["default_branch"]
    head = subprocess.check_output(["git", "rev-parse", "HEAD"], text=True).strip()
    if api(f"commits/{branch}")["sha"] != head:
        print("A newer default-branch commit will handle this handoff")
        return
    tag = f"engine-{source_version}"
    refs = api(f"git/matching-refs/tags/{tag}")
    if not any(ref["ref"] == f"refs/tags/{tag}" for ref in refs):
        obj = api("git/tags", tag=tag, message=f"Engine {source_version}", object=head, type="commit")
        api("git/refs", ref=f"refs/tags/{tag}", sha=obj["sha"])
    commit = api(f"commits/{tag}")["sha"]
    source_matches(commit)
    if published(release(tag)):
        dispatch("adopt-engine.yml", branch, tag)
    else:
        # workflow_dispatch is deliberately used: it runs even with GITHUB_TOKEN,
        # and running at the tag gives the attestation the correct source ref.
        dispatch("engine-release.yml", tag)


def prepare_pin(tag):
    check_tag(tag)
    if tag != f"engine-{engine_version()}":
        print("This engine was superseded; refusing a stale pin PR")
        output("adopt", False)
        return
    source_matches(api(f"commits/{tag}")["sha"])
    old = (ROOT / "engine/release.pin.json").read_bytes()
    pin_release(tag)
    changed = (ROOT / "engine/release.pin.json").read_bytes() != old
    paths = subprocess.check_output(["git", "diff", "--name-only"], text=True).splitlines()
    if paths != (["engine/release.pin.json"] if changed else []):
        raise ValueError("Pin automation may change only engine/release.pin.json")
    output("adopt", changed)


def require_merge_policy(branch):
    rules = api(f"rules/branches/{branch}")
    status_rules = [rule for rule in rules if rule["type"] == "required_status_checks"]
    if status_rules:
        for rule in status_rules:
            checks = rule["parameters"]
            contexts = {item["context"] for item in checks.get("required_status_checks", [])}
            if not checks.get("strict_required_status_checks_policy") or not REQUIRED_CHECKS <= contexts:
                continue
            if rule["ruleset_source_type"] != "Repository" or rule["ruleset_source"] != REPOSITORY:
                continue
            ruleset = api(f"rulesets/{rule['ruleset_id']}")
            if ruleset["enforcement"] == "active" and ruleset.get("bypass_actors") == []:
                return
        raise ValueError("Require up-to-date branches and all release checks in an active repository ruleset with no bypass actors: " + ", ".join(sorted(REQUIRED_CHECKS)))
    # Older repositories can still use classic branch protection instead.
    policy = api(f"branches/{branch}/protection")
    checks = policy.get("required_status_checks") or {}
    contexts = set(checks.get("contexts", [])) | {item["context"] for item in checks.get("checks", [])}
    if not checks.get("strict") or not REQUIRED_CHECKS <= contexts:
        raise ValueError("Enable strict branch protection with required checks: " + ", ".join(sorted(REQUIRED_CHECKS)))
    if not policy.get("enforce_admins", {}).get("enabled"):
        raise ValueError("Branch protection must enforce required checks for administrators too")


def auto_merge(number, kind):
    pr = json.loads(gh("pr", "view", str(number), "--repo", REPOSITORY, "--json",
                       "headRefName,headRefOid,baseRefName,isCrossRepository,state,files"))
    branch = api("")["default_branch"]
    if pr["state"] != "OPEN" or pr["isCrossRepository"] or pr["baseRefName"] != branch:
        raise ValueError("Auto-merge is restricted to open same-repository automation PRs")
    files = {item["path"] for item in pr["files"]}
    if kind == "pin":
        if not re.fullmatch(r"automation/engine-\d+\.\d+\.\d+", pr["headRefName"]) or files != {"engine/release.pin.json"}:
            raise ValueError("Engine adoption PR must change only the pin")
    else:
        allowed = {"CHANGELOG.md", "manifest.json", ".plugin-version", ".release-please-manifest.json"}
        if pr["headRefName"] != f"release-please--branches--{branch}" or not files or not files <= allowed:
            raise ValueError("Unexpected release PR branch or files")
    require_merge_policy(branch)
    gh("pr", "merge", str(number), "--repo", REPOSITORY, "--auto", "--squash",
       "--match-head-commit", pr["headRefOid"])


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    sub = parser.add_subparsers(dest="command", required=True)
    sub.add_parser("route")
    status = sub.add_parser("engine-status")
    status.add_argument("--tag", required=True)
    pin = sub.add_parser("prepare-pin")
    pin.add_argument("--tag", required=True)
    gate = sub.add_parser("ready")
    gate.add_argument("--require", action="store_true")
    merge = sub.add_parser("auto-merge")
    merge.add_argument("--pr", required=True, type=int)
    merge.add_argument("--kind", choices=["pin", "release"], required=True)
    sub.add_parser("auto-release")
    args = parser.parse_args()
    if args.command == "route":
        route()
    elif args.command == "engine-status":
        check_tag(args.tag)
        if args.tag != f"engine-{engine_version()}":
            raise ValueError("Engine tag must match Cargo version")
        output("published", published(release(args.tag)))
    elif args.command == "prepare-pin":
        prepare_pin(args.tag)
    elif args.command == "ready":
        aligned = ready()
        output("ready", aligned)
        if args.require and not aligned:
            raise ValueError("UI release blocked until the published engine pin lands")
    elif args.command == "auto-merge":
        auto_merge(args.pr, args.kind)
    else:
        branch = api("")["default_branch"]
        prs = json.loads(gh("pr", "list", "--repo", REPOSITORY, "--state", "open",
                           "--head", f"release-please--branches--{branch}", "--json", "number"))
        for pr in prs:
            auto_merge(pr["number"], "release")


if __name__ == "__main__":
    main()
