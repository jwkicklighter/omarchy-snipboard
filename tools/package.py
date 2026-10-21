#!/usr/bin/env python3
"""Build a curated release bundle; never include clipboard data or the checkout."""

import argparse
import hashlib
import json
import os
from pathlib import Path
import platform
import re
import select
import subprocess
import tarfile
import tempfile
import tomllib

ROOT = Path(__file__).resolve().parent.parent
PLUGIN_ID = "io.github.jwkicklighter.snipboard"


def check_versions(root=ROOT, tag=None):
    manifest = json.loads((root / "manifest.json").read_text())
    cargo = tomllib.loads((root / "Cargo.toml").read_text())["package"]
    lock = tomllib.loads((root / "Cargo.lock").read_text())
    locked = next(p for p in lock["package"] if p["name"] == cargo["name"])
    version = manifest["version"]
    if manifest["id"] != PLUGIN_ID:
        raise ValueError("Plugin ID must remain stable")
    if not re.fullmatch(r"\d+\.\d+\.\d+", version):
        raise ValueError("Release version must be X.Y.Z")
    if cargo["version"] != version or locked["version"] != version:
        raise ValueError("manifest.json, Cargo.toml and Cargo.lock versions must match")
    if tag is not None and tag != f"v{version}":
        raise ValueError(f"Expected tag v{version}, got {tag}")
    return version


def release_files(root=ROOT):
    # An explicit allowlist prevents accidental publication of personal/local data.
    names = [
        "manifest.json", "Cargo.toml", "Cargo.lock", "README.md", "LICENSE", "preview.png",
        "Snipboard.qml", "SbActions.js", "SbButton.qml", "SbPasteHandoff.qml",
        "SbSnippetEditor.qml", "bin/linux-x86_64/snipboard-worker", "bin/linux-x86_64/worker.json",
    ]
    files = [root / name for name in names] + sorted((root / "docs").glob("*.md"))
    for path in files:
        if path.is_symlink() or not path.is_file():
            raise ValueError(f"Missing or unsafe release file: {path}")
    if not os.access(root / "bin/linux-x86_64/snipboard-worker", os.X_OK):
        raise ValueError("Missing executable bundled worker")
    return files


def smoke_test(archive):
    # Extract only our just-created allowlisted archive, outside the watched tree.
    with tempfile.TemporaryDirectory(prefix="snipboard-release-") as directory:
        base = Path(directory)
        with tarfile.open(archive) as bundle:
            bundle.extractall(base, filter="data")
        worker = base / PLUGIN_ID / "bin/linux-x86_64/snipboard-worker"
        process = subprocess.Popen(
            [str(worker), "--data", str(base / "data")],
            stdin=subprocess.PIPE, stdout=subprocess.PIPE, stderr=subprocess.PIPE,
        )
        try:
            if not select.select([process.stdout], [], [], 15)[0]:
                raise ValueError("Packaged worker timed out before ready")
            ready = json.loads(process.stdout.readline())
            if ready.get("op") != "ready":
                raise ValueError(f"Packaged worker did not report ready: {ready.get('op')}")
            process.stdin.close()
            if process.wait(timeout=15) != 0:
                raise ValueError("Packaged worker failed on shutdown")
        finally:
            if process.poll() is None:
                process.kill()
                process.wait()
            process.stdout.close()
            process.stderr.close()


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--tag")
    parser.add_argument("--check", action="store_true")
    parser.add_argument("--smoke-test", action="store_true")
    args = parser.parse_args()
    version = check_versions(tag=args.tag)
    if args.check:
        print(f"Release metadata valid: v{version}")
        return
    if platform.system() != "Linux" or platform.machine() != "x86_64":
        raise ValueError("This release target supports Linux x86_64 only")
    from bundle import check_bundle
    check_bundle()
    files = release_files()
    destination = ROOT / "build/dist"
    destination.mkdir(parents=True, exist_ok=True)
    archive = destination / f"snipboard-v{version}-linux-x86_64.tar.gz"
    with tarfile.open(archive, "w:gz") as bundle:
        for path in files:
            bundle.add(path, arcname=str(Path(PLUGIN_ID) / path.relative_to(ROOT)), recursive=False)
    if args.smoke_test:
        smoke_test(archive)
    with archive.open("rb") as stream:
        digest = hashlib.file_digest(stream, "sha256").hexdigest()
    archive.with_suffix(archive.suffix + ".sha256").write_text(f"{digest}  {archive.name}\n")
    print(archive)


if __name__ == "__main__":
    main()
