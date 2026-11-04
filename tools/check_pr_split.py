#!/usr/bin/env python3
"""Classify engine paths for versioning and immutable-tag checks."""

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
    from release_policy import main
    main()
