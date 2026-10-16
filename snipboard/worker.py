"""Persistent JSON-lines worker. Search never reads disk or launches processes."""

import argparse
import copy
import fcntl
import json
import os
from pathlib import Path
import signal
import subprocess
import sys
import time
import uuid

from .core import MAX_KEYWORD, render, scratch_match, synthetic_library, synthetic_records, utf16_length
from .native import NativeIndex
from .storage import Store, atomic_json, read_json, validate_library, validate_usage


class LibraryActionError(ValueError):
    """Only fixed, payload-free messages may cross the UI boundary."""


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

    def library_state(self):
        return {"revision": self.store.revision, "readonly": self.store.readonly}

    def snippet(self, record):
        item = self.index.by_id.get(record)
        if not item or item["kind"] != "snippet":
            raise LibraryActionError("Snippet no longer exists. Reopen the editor.")
        return {key: item[key] for key in ("id", "name", "body", "notes", "keyword")}

    def prepare_library(self, library):
        records = [r for r in self.index.records if r["kind"] == "clipboard"]
        records.extend(dict(s, kind="snippet") for s in library["snippets"])
        return NativeIndex(records, self.usage)

    def change_library(self, request):
        if self.store.readonly:
            raise LibraryActionError("Library is read-only. Review storage and explicitly reload before saving.")
        if request.get("revision") != self.store.revision:
            raise LibraryActionError("Library changed since this editor opened. Keep your draft and reopen before saving.")
        record = request.get("record", "")
        if not isinstance(record, str):
            raise LibraryActionError("Invalid snippet identity.")
        if record:
            self.snippet(record)
        library = copy.deepcopy(self.store.library)
        snippets = library["snippets"]
        if request["op"] == "snippet-delete":
            if not record:
                raise LibraryActionError("Select a saved snippet to delete.")
            library["snippets"] = [s for s in snippets if s["id"] != record]
        else:
            fields = request.get("fields")
            if not isinstance(fields, dict) or set(fields) != {"name", "body", "notes", "keyword"}:
                raise LibraryActionError("Name, body, notes, and keyword are required.")
            item = dict(fields, id=record or "snippet-" + uuid.uuid4().hex)
            if record:
                position = next(i for i, s in enumerate(snippets) if s["id"] == record)
                snippets[position] = item
            else:
                snippets.append(item)
            record = item["id"]
        try:
            validate_library(library)
        except ValueError:
            raise LibraryActionError("Invalid snippet: use a nonblank name (256 max), body ≤65,536, notes ≤4,096, and an optional whitespace-free keyword ≤64. Keywords must not duplicate or prefix another; use at most one {cursor}.") from None
        # Prepare first, persist second, publish last: a failed save never changes search.
        fresh = self.prepare_library(library)
        try:
            self.store.save(library)
        except (OSError, ValueError):
            fresh.close()
            raise LibraryActionError("Save could not complete. Draft retained; review storage or external changes before reloading.") from None
        previous, self.index = self.index, fresh
        previous.close()
        return dict(self.library_state(), record=record, deleted=request["op"] == "snippet-delete")

    def dispatch(self, request):
        operation = request.get("op")
        if operation == "snippet-get":
            record = request.get("record", "")
            if not isinstance(record, str):
                raise LibraryActionError("Invalid snippet identity.")
            item = self.snippet(record) if record else {"id": "", "name": "", "body": "", "notes": "", "keyword": ""}
            return dict(self.library_state(), snippet=item)
        if operation in ("snippet-save", "snippet-delete"):
            return self.change_library(request)
        if operation == "library-reload":
            try:
                library = read_json(self.store.library_path, validate_library)
                fresh = self.prepare_library(library)
            except (OSError, ValueError):
                self.store.readonly = True
                raise LibraryActionError("Reload failed. Last valid library is available read-only; existing files were preserved.") from None
            self.store.library, self.store.readonly = library, False
            previous, self.index = self.index, fresh
            previous.close()
            return self.library_state()
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
    print(json.dumps(dict(worker.library_state(), op="ready", count=len(worker.index.records))), flush=True)
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
            except LibraryActionError as error:
                response = dict(worker.library_state(), op=request.get("op"), seq=request.get("seq", 0), error=str(error))
            except (ValueError, KeyError, TypeError, OSError):
                response = {"op": request.get("op"), "seq": request.get("seq", 0),
                            "error": "Action failed; check prototype storage or capability. Existing data was not reset."}
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
