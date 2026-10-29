#!/usr/bin/env python3
"""Install only the plugin's committed worker pin; no Cargo or latest lookup."""

import argparse
import fcntl
import hashlib
import json
import os
from pathlib import Path
import platform
import re
import sys
import tempfile
import urllib.request

ROOT = Path(__file__).resolve().parent.parent
REPOSITORY = "jwkicklighter/omarchy-snipboard"
ASSET = "snipboard-worker-linux-x86_64"
MAX_DOWNLOAD = 64 * 1024 * 1024


def digest(path):
    with path.open("rb") as stream:
        return hashlib.file_digest(stream, "sha256").hexdigest()


def read_pin(path):
    pin = json.loads(path.read_text())
    required = {"repository", "version", "tag", "asset", "sha256"}
    if set(pin) != required:
        raise ValueError("Invalid worker pin fields")
    if pin["repository"] != REPOSITORY or not re.fullmatch(r"\d+\.\d+\.\d+", pin["version"]):
        raise ValueError("Invalid worker repository/version")
    if pin["tag"] != f"engine-{pin['version']}":
        raise ValueError("Worker tag must be engine-X.Y.Z")
    if pin["asset"] != ASSET:
        raise ValueError("Unsupported worker asset")
    if not re.fullmatch(r"[0-9a-f]{64}", pin["sha256"]):
        raise ValueError("Invalid worker checksum")
    return pin


def download(url, destination):
    request = urllib.request.Request(url, headers={"User-Agent": "snipboard-bootstrap"})
    with urllib.request.urlopen(request, timeout=30) as response, destination.open("wb") as output:
        size = 0
        while chunk := response.read(1024 * 1024):
            size += len(chunk)
            if size > MAX_DOWNLOAD:
                raise ValueError("Worker download exceeds size limit")
            output.write(chunk)


def ensure_worker(pin, data_home, fetch=download):
    if platform.system() != "Linux" or platform.machine() != "x86_64":
        raise ValueError("The pinned worker supports Linux x86_64 only")
    expected = pin["sha256"]
    directory = data_home / "omarchy-snipboard" / "workers" / expected
    directory.mkdir(parents=True, exist_ok=True, mode=0o700)
    destination = directory / "snipboard-worker"
    # Concurrent shell instances serialize installation; versions never overwrite each other.
    with (directory / ".install.lock").open("a") as lock:
        fcntl.flock(lock, fcntl.LOCK_EX)
        if not destination.is_symlink() and destination.is_file() and os.access(destination, os.X_OK) and digest(destination) == expected:
            return destination
        with tempfile.TemporaryDirectory(prefix=".install-", dir=directory) as temporary:
            asset = Path(temporary) / "download"
            url = f"https://github.com/{pin['repository']}/releases/download/{pin['tag']}/{pin['asset']}"
            fetch(url, asset)
            if digest(asset) != pin["sha256"]:
                raise ValueError("Worker download checksum mismatch; refusing installation")
            worker = asset
            with worker.open("rb") as stream:
                header = stream.read(20)
            if header[:6] != b"\x7fELF\x02\x01" or header[18:20] != b"\x3e\x00":
                raise ValueError("Worker must be a Linux x86_64 ELF executable")
            worker.chmod(0o755)
            worker.replace(destination)
        return destination


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--check", action="store_true", help="validate pin without downloading")
    args = parser.parse_args()
    pin = read_pin(ROOT / "engine/release.pin.json")
    if args.check:
        print(f"Valid worker pin: {pin['tag']}")
        return
    data_home = Path(os.environ.get("XDG_DATA_HOME", str(Path.home() / ".local/share"))).resolve()
    log = data_home / "omarchy-snipboard/bootstrap.log"
    log.parent.mkdir(parents=True, exist_ok=True, mode=0o700)
    try:
        worker = ensure_worker(pin, data_home)
    except Exception as error:
        log.write_text(f"Could not install {pin['tag']}: {error}\nCheck network access and the published release; reload the plugin to retry.\n")
        log.chmod(0o600)
        print("Worker installation failed. See $XDG_DATA_HOME/omarchy-snipboard/bootstrap.log (default ~/.local/share). Reload to retry.", file=sys.stderr)
        raise SystemExit(1) from None
    log.write_text(f"Worker ready: {pin['tag']}\n")
    log.chmod(0o600)
    print(worker)


if __name__ == "__main__":
    main()
