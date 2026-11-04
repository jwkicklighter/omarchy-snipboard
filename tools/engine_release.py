#!/usr/bin/env python3
"""Package an independent engine release, or verify and adopt a published pin."""

import argparse
import json
import os
from pathlib import Path
import re
import shutil
import subprocess
import tempfile
import tomllib

from worker_runtime import ASSET, REPOSITORY, ROOT, digest, download, read_pin


def engine_version(root=ROOT):
    package = tomllib.loads((root / "Cargo.toml").read_text())["package"]
    locked = next(p for p in tomllib.loads((root / "Cargo.lock").read_text())["package"] if p["name"] == package["name"])
    version = package["version"]
    if not re.fullmatch(r"\d+\.\d+\.\d+", version) or locked["version"] != version:
        raise ValueError("Engine Cargo versions must match and use X.Y.Z")
    return version


def check_executable(path):
    if path.is_symlink() or not path.is_file() or not os.access(path, os.X_OK):
        raise ValueError("Worker must be a regular executable file")
    with path.open("rb") as executable:
        header = executable.read(20)
    if header[:6] != b"\x7fELF\x02\x01" or header[18:20] != b"\x3e\x00":
        raise ValueError("Worker must be a Linux x86_64 ELF executable")
    versions = subprocess.check_output(["readelf", "--version-info", str(path)], text=True)
    required = [tuple(map(int, match.split("."))) for match in re.findall(r"GLIBC_(\d+(?:\.\d+)+)", versions)]
    if required and max(required) > (2, 39):
        raise ValueError("Build release workers on Ubuntu 24.04 (glibc <= 2.39)")


def package_engine(worker, destination, tag, root=ROOT):
    version = engine_version(root)
    if tag != f"engine-{version}":
        raise ValueError(f"Expected engine-{version}, got {tag}")
    check_executable(worker)
    destination.mkdir(parents=True, exist_ok=True)
    shutil.copy2(worker, destination / ASSET)
    pin = {"repository": REPOSITORY, "version": version, "tag": tag, "asset": ASSET, "sha256": digest(worker)}
    metadata = {
        "version": version, "target": "x86_64-unknown-linux-gnu", "sha256": pin["sha256"],
        "commit": subprocess.check_output(["git", "rev-parse", "HEAD"], cwd=root, text=True).strip(),
    }
    (destination / "worker.json").write_text(json.dumps(metadata, indent=2) + "\n")
    (destination / "release.pin.json").write_text(json.dumps(pin, indent=2) + "\n")
    names = (ASSET, "worker.json", "release.pin.json")
    (destination / "SHA256SUMS").write_text("".join(f"{digest(destination / name)}  {name}\n" for name in names))


def gh(*args):
    return subprocess.check_output(["gh", *args], text=True).strip()


def verify_provenance(worker, tag, commit):
    # gh validates the signature, executable digest and certificate-backed source
    # identity. Do not trust worker.json (or unsigned predicate fields) for this.
    gh("attestation", "verify", str(worker), "--repo", REPOSITORY,
       "--signer-workflow", f"{REPOSITORY}/.github/workflows/engine-release.yml",
       "--source-digest", commit, "--source-ref", f"refs/tags/{tag}",
       "--predicate-type", "https://slsa.dev/provenance/v1",
       "--deny-self-hosted-runners")


def pin_release(tag, destination=ROOT / "engine/release.pin.json"):
    if not re.fullmatch(r"engine-\d+\.\d+\.\d+", tag):
        raise ValueError("Expected engine-X.Y.Z")
    release = json.loads(gh("release", "view", tag, "--repo", REPOSITORY, "--json", "isDraft,isPrerelease,tagName"))
    if release["isDraft"] or release["isPrerelease"] or release["tagName"] != tag:
        raise ValueError("Pin only public, published stable engine releases")
    commit = gh("api", f"repos/{REPOSITORY}/commits/{tag}", "--jq", ".sha")
    with tempfile.TemporaryDirectory(prefix="snipboard-pin-") as temporary:
        directory = Path(temporary)
        for name in ("release.pin.json", "worker.json", ASSET, "SHA256SUMS"):
            download(f"https://github.com/{REPOSITORY}/releases/download/{tag}/{name}", directory / name)
        pin = read_pin(directory / "release.pin.json")
        if pin["tag"] != tag:
            raise ValueError("Candidate pin does not match release")
        checksums = (directory / "SHA256SUMS").read_text()
        expected = "".join(f"{digest(directory / name)}  {name}\n" for name in (ASSET, "worker.json", "release.pin.json"))
        metadata = json.loads((directory / "worker.json").read_text())
        if checksums != expected or metadata != {
            "version": pin["version"], "target": "x86_64-unknown-linux-gnu", "sha256": pin["sha256"], "commit": commit,
        } or digest(directory / ASSET) != pin["sha256"]:
            raise ValueError("Release checksums/build metadata do not match")
        (directory / ASSET).chmod(0o755)
        check_executable(directory / ASSET)
        verify_provenance(directory / ASSET, tag, commit)
        # Stage next to the tracked pin: a failed verification never changes it.
        with tempfile.NamedTemporaryFile(dir=destination.parent, delete=False) as staged:
            path = Path(staged.name)
            try:
                staged.write((directory / "release.pin.json").read_bytes())
                staged.flush()
                os.fsync(staged.fileno())
                path.chmod(0o644)
                path.replace(destination)
            finally:
                path.unlink(missing_ok=True)


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    sub = parser.add_subparsers(dest="command", required=True)
    build = sub.add_parser("package")
    build.add_argument("--worker", type=Path, default=ROOT / "build/release/snipboard-worker")
    build.add_argument("--destination", type=Path, default=ROOT / "build/engine-dist")
    build.add_argument("--tag", required=True)
    build.add_argument("--root", type=Path, default=ROOT, help="engine source checkout")
    pin = sub.add_parser("pin")
    pin.add_argument("tag")
    verify = sub.add_parser("verify", help="Verify provenance of a built release executable")
    verify.add_argument("--worker", type=Path, required=True)
    verify.add_argument("--tag", required=True)
    args = parser.parse_args()
    if args.command == "package":
        package_engine(args.worker, args.destination, args.tag, args.root)
    elif args.command == "pin":
        pin_release(args.tag)
    else:
        if not re.fullmatch(r"engine-\d+\.\d+\.\d+", args.tag):
            raise ValueError("Expected engine-X.Y.Z")
        commit = gh("api", f"repos/{REPOSITORY}/commits/{args.tag}", "--jq", ".sha")
        verify_provenance(args.worker, args.tag, commit)


if __name__ == "__main__":
    main()
