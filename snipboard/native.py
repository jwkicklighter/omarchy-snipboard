"""Small C ABI boundary, loaded only into the isolated worker process."""
import ctypes
import json
from pathlib import Path
import time


class NativeIndex:
    def __init__(self, records, usage=None):
        self.records = records
        self.by_id = {r["id"]: r for r in records}
        self.usage = usage if usage is not None else {}
        self.library = ctypes.CDLL(str(Path(__file__).resolve().parent.parent / "build/libsnipboard-search.so"))
        self.library.snipboard_create.argtypes = [ctypes.c_char_p]
        self.library.snipboard_create.restype = ctypes.c_void_p
        self.library.snipboard_destroy.argtypes = [ctypes.c_void_p]
        self.library.snipboard_destroy.restype = None
        self.library.snipboard_usage.argtypes = [ctypes.c_void_p, ctypes.c_char_p]
        self.library.snipboard_usage.restype = None
        self.library.snipboard_search.argtypes = [ctypes.c_void_p, ctypes.c_char_p]
        self.library.snipboard_search.restype = ctypes.c_char_p
        # Payloads are already strings; only bounded metadata crosses this boundary.
        metadata = [dict(r, **{key: r.get(key, "").casefold()[:512]
                              for key in ("keyword", "name", "notes", "body")}) for r in records]
        self.handle = self.library.snipboard_create(json.dumps(metadata, ensure_ascii=True).encode())
        self.update_usage()

    def close(self):
        if self.handle:
            self.library.snipboard_destroy(self.handle)
            self.handle = None

    def update_usage(self):
        self.library.snipboard_usage(self.handle, json.dumps(self.usage).encode())

    def search(self, query, kind="all", limit=60, now=None):
        if not isinstance(query, str) or len(query) > 256 or kind not in ("all", "clipboard", "snippet"):
            raise ValueError("invalid search")
        request = {"query": query.strip().casefold(), "kind": kind, "limit": limit, "now": time.time() if now is None else now}
        ids = json.loads(self.library.snipboard_search(self.handle, json.dumps(request, ensure_ascii=True).encode()))
        return [self.by_id[id] for id in ids]
