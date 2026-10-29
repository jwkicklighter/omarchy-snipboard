#!/usr/bin/env python3
"""Build a curated release bundle; never include clipboard data or the checkout."""

import argparse
import hashlib
import json
import os
from pathlib import Path
import platform
import re
import subprocess
import tarfile
import tempfile

ROOT = Path(__file__).resolve().parent.parent
PLUGIN_ID = "io.github.jwkicklighter.snipboard"


def check_versions(root=ROOT, tag=None):
    manifest = json.loads((root / "manifest.json").read_text())
    version = manifest["version"]
    if (root / ".plugin-version").read_text().strip() != version:
        raise ValueError("manifest.json and .plugin-version must match")
    if manifest["id"] != PLUGIN_ID:
        raise ValueError("Plugin ID must remain stable")
    if not re.fullmatch(r"\d+\.\d+\.\d+", version):
        raise ValueError("Release version must be X.Y.Z")
    if tag is not None and tag != f"v{version}":
        raise ValueError(f"Expected tag v{version}, got {tag}")
    return version


def release_files(root=ROOT):
    # An explicit allowlist prevents accidental publication of personal/local data.
    names = [
        "manifest.json", ".plugin-version", "README.md", "LICENSE", "preview.png",
        "Snipboard.qml", "SbActions.js", "SbButton.qml", "SbPasteHandoff.qml", "SbPreview.qml",
        "SbSnippetEditor.qml", "engine/release.pin.json", "tools/worker_runtime.py",
    ]
    files = [root / name for name in names] + sorted((root / "docs").glob("*.md"))
    for path in files:
        if path.is_symlink() or not path.is_file():
            raise ValueError(f"Missing or unsafe release file: {path}")
    return files


def smoke_test(archive):
    # Extract only our just-created allowlisted archive, outside the watched tree.
    with tempfile.TemporaryDirectory(prefix="snipboard-release-") as directory:
        base = Path(directory)
        with tarfile.open(archive) as bundle:
            bundle.extractall(base, filter="data")
        # Exercise the exact shipped bootstrap/pin, never a source-built candidate.
        environment = dict(os.environ, XDG_DATA_HOME=str(base / "installed"))
        worker = subprocess.check_output(
            ["python3", str(base / PLUGIN_ID / "tools/worker_runtime.py")],
            env=environment, text=True, timeout=120,
        ).strip()
        from worker_smoke import smoke
        smoke(worker)


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
    from worker_runtime import read_pin
    read_pin(ROOT / "engine/release.pin.json")
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
