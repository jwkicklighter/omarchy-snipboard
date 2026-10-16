"""Versioned portable definitions and separate machine-local usage, never history."""

import json
import math
import os
from pathlib import Path
import re
import stat
import tempfile

from .core import MAX_BODY, validate_keywords

MAX_FILE = 8 * 1024 * 1024
ID = re.compile(r"[A-Za-z0-9][A-Za-z0-9._-]{0,127}\Z")


def validate_library(value):
    if not isinstance(value, dict) or set(value) != {"schemaVersion", "snippets", "portable"}:
        raise ValueError("invalid library structure")
    if type(value["schemaVersion"]) is not int or value["schemaVersion"] != 1:
        raise ValueError("unsupported schema")
    snippets = value["snippets"]
    if not isinstance(snippets, list) or len(snippets) > 10000:
        raise ValueError("invalid snippet collection")
    ids = set()
    for snippet in snippets:
        if not isinstance(snippet, dict) or set(snippet) != {"id", "name", "body", "notes", "keyword"}:
            raise ValueError("invalid snippet fields")
        for key, maximum in (("id", 128), ("name", 256), ("body", MAX_BODY), ("notes", 4096), ("keyword", 64)):
            if not isinstance(snippet[key], str) or len(snippet[key]) > maximum or "\x00" in snippet[key]:
                raise ValueError("invalid snippet value")
            snippet[key].encode("utf-8")
        if not ID.fullmatch(snippet["id"]) or snippet["id"] in ids or not snippet["name"]:
            raise ValueError("invalid or duplicate stable id")
        ids.add(snippet["id"])
    if value["portable"] != {"dateFormat": "YYYY-MM-DD"}:
        raise ValueError("unsupported portable settings")
    validate_keywords(snippets)
    return value


def validate_usage(value):
    if not isinstance(value, dict) or len(value) > 10000:
        raise ValueError("invalid usage")
    for key, entry in value.items():
        if not ID.fullmatch(key) or not isinstance(entry, dict) or set(entry) != {"count", "last"}:
            raise ValueError("invalid usage entry")
        if type(entry["count"]) is not int or not 0 <= entry["count"] <= 10**12:
            raise ValueError("invalid usage count")
        if type(entry["last"]) not in (int, float) or not math.isfinite(entry["last"]) or entry["last"] < 0:
            raise ValueError("invalid usage timestamp")
    return value


def read_json(path, validator):
    fd = os.open(path, os.O_RDONLY | os.O_NOFOLLOW)
    with os.fdopen(fd, "rb") as stream:
        info = os.fstat(stream.fileno())
        if not stat.S_ISREG(info.st_mode) or info.st_uid != os.getuid() or info.st_mode & 0o077:
            raise ValueError("data must be an owner-only regular file")
        raw = stream.read(MAX_FILE + 1)
    if len(raw) > MAX_FILE:
        raise ValueError("file exceeds limit")
    def pairs(items):
        result = {}
        for key, val in items:
            if key in result:
                raise ValueError("duplicate JSON key")
            result[key] = val
        return result
    return validator(json.loads(raw, object_pairs_hook=pairs))


def atomic_json(path, value, validator):
    validator(value)
    encoded = (json.dumps(value, ensure_ascii=False, allow_nan=False, indent=2) + "\n").encode()
    if len(encoded) > MAX_FILE:
        raise ValueError("file exceeds limit")
    path = Path(path)
    if path.is_symlink():
        raise ValueError("refusing symlink")
    fd, temporary = tempfile.mkstemp(prefix=".snipboard-", dir=path.parent)
    try:
        with os.fdopen(fd, "wb") as stream:
            stream.write(encoded)
            stream.flush()
            os.fsync(stream.fileno())
        os.replace(temporary, path)
        directory = os.open(path.parent, os.O_RDONLY | os.O_DIRECTORY)
        try:
            os.fsync(directory)
        finally:
            os.close(directory)
    finally:
        if os.path.exists(temporary):
            os.unlink(temporary)


class Store:
    def __init__(self, root):
        self.root = Path(root)
        self.root.mkdir(mode=0o700, parents=True, exist_ok=True)
        info = self.root.lstat()
        if not stat.S_ISDIR(info.st_mode) or info.st_uid != os.getuid() or info.st_mode & 0o077:
            raise ValueError("prototype root must be an owner-only real directory")
        self.library_path = self.root / "library.json"
        self.usage_path = self.root / "usage.local.json"
        self.library = None
        self.readonly = False

    def initialize(self, fixture):
        marker = self.root / "initialized"
        if not marker.exists() and not self.library_path.exists():
            atomic_json(self.library_path, fixture, validate_library)
            marker.touch(mode=0o600, exist_ok=False)
        return self.reload()

    def reload(self):
        try:
            fresh = read_json(self.library_path, validate_library)
        except (OSError, ValueError):
            self.readonly = True
            raise
        self.library, self.readonly = fresh, False
        return fresh

    def save(self, library):
        if self.readonly:
            raise ValueError("storage is read-only after a failed reload")
        current = read_json(self.library_path, validate_library)
        if current != self.library:
            self.readonly = True
            raise ValueError("external change requires explicit reload/review")
        atomic_json(self.library_path, library, validate_library)
        self.library = library
