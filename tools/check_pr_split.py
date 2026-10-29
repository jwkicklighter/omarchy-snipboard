#!/usr/bin/env python3
"""Keep engine implementation and plugin delivery changes in separate PRs."""

import subprocess
import sys


def groups(paths):
    engine = []
    ui = []
    for path in paths:
        if path.startswith(("src/", ".cargo/")) or path in ("Cargo.toml", "Cargo.lock", "tests/worker.rs", "tests/layouts.rs", "tests/paste_lua.rs"):
            engine.append(path)
        if path.endswith((".qml", ".js")) or path in ("manifest.json", "engine/release.pin.json", "tools/worker_runtime.py", "tests/qml.rs", "tests/overlay.rs"):
            ui.append(path)
    return engine, ui


if __name__ == "__main__":
    # Include both sides of renames: moving a file cannot evade the boundary.
    paths = subprocess.check_output(["git", "diff", "--name-only", "--no-renames", "-z", sys.argv[1], "HEAD"], text=True).split("\0")
    engine, ui = groups(paths)
    if engine and ui:
        sys.exit("Split engine and UI changes: merge/release the engine first, then submit compatible UI and its published pin. See docs/releases.md.\nEngine: " + ", ".join(engine) + "\nUI: " + ", ".join(ui))
