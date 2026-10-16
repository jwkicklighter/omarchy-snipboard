"""Pure search, templates, and bounded keyword matching. No desktop access."""

from dataclasses import dataclass
from datetime import datetime
import heapq
import math
import re
import time
import unicodedata

MAX_KEYWORD = 64
MAX_BODY = 65536
MAX_CLIPBOARD = 16384
VARIABLE = re.compile(r"\{\{(date|time|clipboard|cursor)\}\}|\{(date|time|clipboard|cursor)\}")


def validate_keywords(snippets):
    keywords = []
    for item in snippets:
        keyword = item.get("keyword", "")
        if not keyword:
            continue
        if (not isinstance(keyword, str) or len(keyword) > MAX_KEYWORD
                or any(c.isspace() or unicodedata.category(c)[0] == "C" for c in keyword)):
            raise ValueError("invalid keyword")
        keywords.append(keyword)
    keywords.sort()
    if any(b.startswith(a) for a, b in zip(keywords, keywords[1:])):
        raise ValueError("duplicate or prefix-conflicting keywords")


@dataclass(frozen=True)
class Rendered:
    text: str
    cursor: int | None


def render(body, clipboard="", now=None):
    """Single pass: clipboard content is literal; {{date}} escapes a token."""
    if len(body) > MAX_BODY or len(clipboard) > MAX_CLIPBOARD:
        raise ValueError("template or clipboard exceeds limit")
    now = now or datetime.now()
    values = {"date": now.strftime("%Y-%m-%d"), "time": now.strftime("%H:%M"), "clipboard": clipboard}
    parts, length, cursor, previous = [], 0, None, 0
    for match in VARIABLE.finditer(body):
        literal = body[previous:match.start()]
        parts.append(literal)
        length += len(literal)
        if match[1]:
            value = "{" + match[1] + "}"
        elif match[2] == "cursor":
            if cursor is not None:
                raise ValueError("multiple cursor markers")
            cursor, value = length, ""
        else:
            value = values[match[2]]
        parts.append(value)
        length += len(value)
        previous = match.end()
    parts.append(body[previous:])
    text = "".join(parts)
    if len(text) > MAX_BODY:
        raise ValueError("rendered text exceeds limit")
    return Rendered(text, cursor)


def word_character(char):
    return char == "_" or char.isalnum() or unicodedata.category(char).startswith("M")


class Matcher:
    """Disabled by default, bounded and ephemeral; unknown context is not a boundary."""

    def __init__(self, snippets):
        validate_keywords(snippets)
        self.keywords = {s["keyword"]: s["id"] for s in snippets if s.get("keyword")}
        self.enabled = False
        self.reset()

    def reset(self):
        self.buffer = ""
        self.context = None
        self.boundary = False

    def feed(self, text, context, *, allowed=False, injected=False, boundary=False):
        if not self.enabled or not allowed or injected or context is None:
            self.reset()
            return None
        if context != self.context:
            self.reset()
            self.context = context
            self.boundary = boundary
        for char in text:
            candidate = self.buffer + char
            if self.boundary and candidate in self.keywords:
                result = self.keywords[candidate]
                self.reset()
                return result
            if self.boundary and any(k.startswith(candidate) for k in self.keywords):
                self.buffer = candidate[:MAX_KEYWORD]
            else:
                self.buffer = ""
                self.boundary = not word_character(char)
        return None


def scratch_match(prefix, snippets):
    """Only inspect a bounded prefix of an explicitly trusted scratch TextEdit."""
    for snippet in snippets:
        keyword = snippet.get("keyword", "")
        if keyword and prefix.endswith(keyword):
            preceding = prefix[:-len(keyword)]
            if not preceding or not word_character(preceding[-1]):
                return snippet
    return None


def utf16_length(text):
    return len(text.encode("utf-16-le")) // 2


def usage_score(stat, now):
    age = max(0, now - stat.get("last", 0)) / 86400
    return math.log1p(stat.get("count", 0)) * math.exp(-age / 14)


def field_match(query, value):
    if query == value:
        return (0, 0)
    if value.startswith(query):
        return (1, len(value) - len(query))
    offset = value.find(query)
    if offset >= 0:
        return (2, offset)
    position, first = -1, -1
    for char in query:
        position = value.find(char, position + 1)
        if position < 0:
            return None
        if first < 0:
            first = position
    return (3, position - first - len(query) + 1)


class Index:
    def __init__(self, records, usage=None):
        self.records = records
        self.by_id = {r["id"]: r for r in records}
        self.usage = usage if usage is not None else {}
        self.fields = []
        for record in records:
            fields = tuple(record.get(key, "").casefold()[:512]
                           for key in ("keyword", "name", "notes", "body"))
            self.fields.append((record, fields, frozenset("".join(fields))))

    def search(self, query, kind="all", limit=60, now=None):
        if kind not in ("all", "clipboard", "snippet") or len(query) > 256:
            raise ValueError("invalid search")
        query = query.strip().casefold()
        chars = frozenset(query)
        now = time.time() if now is None else now
        candidates = []
        for record, fields, available in self.fields:
            if kind != "all" and record["kind"] != kind:
                continue
            if query:
                if not chars <= available:
                    continue
                best = None
                for field, value in enumerate(fields):
                    match = field_match(query, value)
                    if match is not None:
                        score = (0 if field < 2 else 1, match[0], field, match[1])
                        if best is None or score < best:
                            best = score
                if best is None:
                    continue
                textual = best
            else:
                textual = (0 if record["kind"] == "clipboard" else 1, 0, 0, 0)
            learned = (usage_score(self.usage.get(record["id"], {}), now)
                       if record["kind"] == "snippet" else 0)
            chronology = -record.get("created", 0) if record["kind"] == "clipboard" else 0
            candidates.append(((*textual, -learned, chronology, record["id"]), record))
        if not query and kind == "all":
            clips = heapq.nsmallest(limit // 2, (c for c in candidates if c[1]["kind"] == "clipboard"))
            snippets = heapq.nsmallest(limit // 2, (c for c in candidates if c[1]["kind"] == "snippet"))
            return [r for _, r in clips + snippets]
        return [r for _, r in heapq.nsmallest(limit, candidates)]


def synthetic_library(count=1000):
    special = [("Email", "!email", "scratch@example.invalid"),
               ("Unicode", "!unicode", "Hello — Καλημέρα 🌍"),
               ("Multiline", "!multi", "First line\nSecond line\nThird line"),
               ("Variables", "!vars", "{date} {time}\n{clipboard}\nBefore {cursor}after")]
    snippets = []
    for i in range(count):
        name, keyword, body = (special[i] if i < len(special) else
                              (f"Project note {i:04d}", "", f"Synthetic reusable project note {i:04d} amber river"))
        snippets.append({"id": f"snippet-{i:04d}", "name": name, "keyword": keyword,
                         "body": body, "notes": "Synthetic prototype fixture"})
    return {"schemaVersion": 1, "snippets": snippets, "portable": {"dateFormat": "YYYY-MM-DD"}}


def synthetic_records(library, count=10000):
    records = []
    for i in range(count):
        body = f"Synthetic clipboard {i:05d} amber river project meeting notes"
        if i % 1000 == 0:
            body += "\n" + "Large synthetic payload. " * 2000
        records.append({"id": f"clipboard-{i:05d}", "kind": "clipboard",
                        "name": f"Clipboard {i:05d}", "body": body, "created": i})
    records.extend(dict(s, kind="snippet") for s in library["snippets"])
    return records
