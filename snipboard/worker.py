"""Persistent JSON-lines worker. Search never reads disk or launches processes."""

import argparse
import fcntl
import json
import os
from pathlib import Path
import signal
import subprocess
import sys
import time

from .core import MAX_KEYWORD, render, scratch_match, synthetic_library, synthetic_records, utf16_length
from .native import NativeIndex
from .storage import Store, atomic_json, read_json, validate_usage


class Worker:
    def __init__(self, root):
        root = Path(root)
        source = Path(__file__).resolve().parent.parent
        if not root.is_absolute() or root.resolve().is_relative_to(source):
            raise ValueError("prototype data must be outside source at an absolute path")
        self.store = Store(root)
        self.lock = open(self.store.root / "worker.lock", "a", opener=lambda path, flags: os.open(path, flags | os.O_NOFOLLOW, 0o600))
        fcntl.flock(self.lock, fcntl.LOCK_EX | fcntl.LOCK_NB)
        library = self.store.initialize(synthetic_library())
        self.usage = read_json(self.store.usage_path, validate_usage) if self.store.usage_path.exists() else {}
        self.index = NativeIndex(synthetic_records(library), self.usage)
        self.child = None

    def cleanup(self):
        if self.child is not None and self.child.poll() is None:
            self.child.terminate()
            try:
                self.child.wait(timeout=1)
            except subprocess.TimeoutExpired:
                self.child.kill()
                self.child.wait()
        self.child = None

    def dispatch(self, request):
        operation = request.get("op")
        if operation == "search":
            start = time.perf_counter_ns()
            rows = self.index.search(request.get("query", ""), request.get("kind", "all"))
            elapsed = (time.perf_counter_ns() - start) / 1e6
            return {"rows": [{"id": r["id"], "name": r["name"], "kind": r["kind"],
                              "summary": r["body"][:100].replace("\n", " ↵ ")} for r in rows], "matchMs": elapsed}
        if operation == "preview":
            item = self.index.by_id[request["record"]]
            return {"body": item["body"], "keyword": item.get("keyword", "")}
        if operation == "scratch":
            if request.get("enabled") is not True or request.get("context") != "trusted-scratch":
                return {"expansion": None}
            prefix = request.get("prefix", "")
            if len(prefix) > MAX_KEYWORD + 1:
                raise ValueError("scratch prefix exceeds bound")
            snippet = scratch_match(prefix, self.store.library["snippets"])
            if not snippet:
                return {"expansion": None}
            result = render(snippet["body"], request.get("clipboard", ""))
            return {"expansion": {"record": snippet["id"], "text": result.text,
                                  "remove": utf16_length(snippet["keyword"]),
                                  "cursor": utf16_length(result.text[:result.cursor]) if result.cursor is not None else utf16_length(result.text)}}
        if operation == "copy":
            item = self.index.by_id[request["record"]]
            result = render(item["body"], request.get("clipboard", ""))
            self.cleanup()
            self.child = subprocess.Popen(["setpriv", "--pdeathsig", "TERM", "wl-copy", "--foreground", "--type", "text/plain;charset=utf-8"],
                                          stdin=subprocess.PIPE, stdout=subprocess.DEVNULL, stderr=subprocess.DEVNULL)
            self.child.stdin.write(result.text.encode())
            self.child.stdin.close()
            return {"message": "Copy requested (clipboard ownership unacknowledged).", "confirmed": False}
        if operation == "used":
            item = self.index.by_id[request["record"]]
            if item["kind"] == "snippet":
                previous = self.usage.get(item["id"], {"count": 0, "last": 0})
                fresh = dict(self.usage)
                fresh[item["id"]] = {"count": previous["count"] + 1, "last": time.time()}
                atomic_json(self.store.usage_path, fresh, validate_usage)
                self.usage.clear()
                self.usage.update(fresh)
                self.index.update_usage()
            return {"ok": True}
        if operation == "render":
            item = self.index.by_id[request["record"]]
            result = render(item["body"], request.get("clipboard", ""))
            return {"text": result.text, "cursor": utf16_length(result.text[:result.cursor]) if result.cursor is not None else utf16_length(result.text)}
        if operation == "shutdown":
            self.cleanup()
            return {"ok": True}
        raise ValueError("unsupported operation")


def serve(root):
    worker = Worker(root)
    def terminate(*_):
        raise SystemExit(0)
    signal.signal(signal.SIGTERM, terminate)
    signal.signal(signal.SIGINT, terminate)
    print(json.dumps({"op": "ready", "count": len(worker.index.records)}), flush=True)
    try:
        while True:
            line = sys.stdin.buffer.readline(512 * 1024 + 1)
            if not line:
                break
            if len(line) > 512 * 1024 or not line.endswith(b"\n"):
                break
            request = {}
            try:
                parsed = json.loads(line)
                if not isinstance(parsed, dict):
                    raise ValueError("request must be an object")
                request = parsed
                response = worker.dispatch(request)
                response.update({"op": request.get("op"), "seq": request.get("seq", 0)})
            except (ValueError, KeyError, TypeError, OSError):
                response = {"op": request.get("op"), "seq": request.get("seq", 0),
                            "error": "Action failed; check prototype storage or capability. No data overwritten."}
            print(json.dumps(response, ensure_ascii=True, separators=(",", ":")), flush=True)
            if request.get("op") == "shutdown":
                break
    finally:
        worker.cleanup()
        worker.index.close()
        worker.lock.close()


if __name__ == "__main__":
    parser = argparse.ArgumentParser()
    parser.add_argument("--data", type=Path, default=Path(os.environ.get("XDG_STATE_HOME") or str(Path.home() / ".local/state")) / "omarchy-snipboard-prototype")
    args = parser.parse_args()
    try:
        serve(args.data)
    except (OSError, ValueError):
        print(json.dumps({"op": "fatal", "error": "Prototype storage unavailable or invalid; existing files preserved."}), flush=True)
        sys.exit(1)
