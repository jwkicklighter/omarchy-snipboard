#!/usr/bin/env python3
"""Maintain the checked-in Linux x86_64 worker; never download or install at runtime."""

import argparse
import hashlib
import json
import os
from pathlib import Path
import re
import subprocess
import tempfile

from package import ROOT, check_versions

DIRECTORY = Path("bin/linux-x86_64")
WORKER = DIRECTORY / "snipboard-worker"
METADATA = DIRECTORY / "worker.json"


def digest(path):
    return hashlib.sha256(path.read_bytes()).hexdigest()


def source_digest(root=ROOT):
    # Include all Rust modules and include_str! payloads, plus Cargo inputs.
    paths = [root / name for name in ("Cargo.toml", "Cargo.lock", ".cargo/config.toml")]
    paths += sorted(path for path in (root / "src").rglob("*") if path.is_file())
    result = hashlib.sha256()
    for path in paths:
        result.update(path.relative_to(root).as_posix().encode() + b"\0")
        result.update(path.read_bytes() + b"\0")
    return result.hexdigest()


def check_executable(path):
    if path.is_symlink() or not path.is_file() or not os.access(path, os.X_OK):
        raise ValueError("Bundled worker must be a regular executable file")
    with path.open("rb") as executable:
        header = executable.read(20)
    if header[:6] != b"\x7fELF\x02\x01" or header[18:20] != b"\x3e\x00":
        raise ValueError("Bundled worker must be a Linux x86_64 ELF executable")
    # Do not accidentally ship an Arch-built executable needing a newer libc
    # than the Ubuntu 24.04 release builder. readelf does not execute the file.
    versions = subprocess.run(
        ["readelf", "--version-info", str(path)], check=True, capture_output=True, text=True,
    ).stdout
    required = [tuple(map(int, match.split("."))) for match in re.findall(r"GLIBC_(\d+(?:\.\d+)+)", versions)]
    if required and max(required) > (2, 39):
        raise ValueError("Build the shipping worker on Ubuntu 24.04 (glibc <= 2.39)")


def check_bundle(root=ROOT):
    worker = root / WORKER
    check_executable(worker)
    metadata = json.loads((root / METADATA).read_text())
    expected = {
        "version": check_versions(root),
        "target": "x86_64-unknown-linux-gnu",
        "sha256": digest(worker),
        "source_sha256": source_digest(root),
    }
    if metadata != expected:
        raise ValueError("Bundled worker is stale or corrupt; rebuild with tools/bundle.py --write --worker PATH")
    return metadata


def write_bundle(worker, root=ROOT):
    check_executable(worker)
    version = check_versions(root)
    destination = root / WORKER
    destination.parent.mkdir(parents=True, exist_ok=True)
    if worker.resolve() != destination.resolve():
        # Replacing the inode also works while the installed worker is running.
        # Never truncate a live executable or leave a partially copied binary.
        atomic_write(destination, worker.read_bytes(), 0o755)
    metadata = {
        "version": version,
        "target": "x86_64-unknown-linux-gnu",
        "sha256": digest(destination),
        "source_sha256": source_digest(root),
    }
    atomic_write(root / METADATA, (json.dumps(metadata, indent=2) + "\n").encode(), 0o644)
    check_bundle(root)


def atomic_write(path, contents, mode):
    with tempfile.NamedTemporaryFile(dir=path.parent, prefix=".worker-", delete=False) as temporary:
        staged = Path(temporary.name)
        try:
            temporary.write(contents)
            temporary.flush()
            os.fsync(temporary.fileno())
            staged.chmod(mode)
            staged.replace(path)
        finally:
            staged.unlink(missing_ok=True)


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--write", action="store_true")
    parser.add_argument("--worker", type=Path)
    args = parser.parse_args()
    if args.write:
        if not args.worker:
            parser.error("--write requires --worker PATH")
        write_bundle(args.worker)
    else:
        if args.worker:
            parser.error("--worker requires --write")
        check_bundle()
    print("Bundled worker metadata, source fingerprint and executable valid")


if __name__ == "__main__":
    main()
