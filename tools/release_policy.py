#!/usr/bin/env python3
"""Allow combined features, but keep published engine adoption a pin-only PR."""

import json
import subprocess
import sys
import tomllib

from check_pr_split import groups
from engine_release import engine_version
from worker_runtime import ROOT, read_pin


def version(value):
    return tuple(map(int, value.split(".")))


def ready(root=ROOT):
    return engine_version(root) == read_pin(root / "engine/release.pin.json")["version"]


def validate(paths, before_version, after_version, before_pin, after_pin):
    engine, _ = groups(paths)
    if version(after_version) < version(before_version):
        raise ValueError("Engine versions cannot go backwards")
    if engine and version(after_version) <= version(before_version):
        raise ValueError("Engine changes require a new Cargo version (Cargo.toml and Cargo.lock)")
    if version(after_version) < version(after_pin["version"]):
        raise ValueError("Source engine version cannot be older than the pin")
    if "engine/release.pin.json" in paths:
        if set(paths) != {"engine/release.pin.json"}:
            raise ValueError("Adopt a published engine in a pin-only PR, after the combined feature PR")
        if after_pin["version"] != after_version:
            raise ValueError("The new pin must match the current source engine version")
        if version(after_pin["version"]) <= version(before_pin["version"]):
            raise ValueError("A pin update must adopt a newer engine version")
    if set(paths) & {".plugin-version", ".release-please-manifest.json"}:
        if after_pin["version"] != after_version:
            raise ValueError("UI releases must wait for the published engine pin")


def main():
    base = subprocess.check_output(["git", "merge-base", sys.argv[1], "HEAD"], text=True).strip()
    paths = subprocess.check_output(["git", "diff", "--name-only", "--no-renames", "-z", base, "HEAD"], text=True).strip("\0").split("\0")
    def previous(path):
        return subprocess.check_output(["git", "show", f"{base}:{path}"], text=True)
    validate(paths, tomllib.loads(previous("Cargo.toml"))["package"]["version"],
             engine_version(), json.loads(previous("engine/release.pin.json")),
             read_pin(ROOT / "engine/release.pin.json"))


if __name__ == "__main__":
    main()
