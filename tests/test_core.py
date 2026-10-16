from datetime import datetime
import unittest

from snipboard.core import (Index, Matcher, render, scratch_match, synthetic_library,
                           synthetic_records, utf16_length, validate_keywords)


def snippet(id, name="Email", body="text", keyword=""):
    return dict(id=id, kind="snippet", name=name, body=body, keyword=keyword, notes="")


class SearchTests(unittest.TestCase):
    def test_exact_relevance_before_usage(self):
        index = Index([snippet("a", "Email"), snippet("b", "Email address"), snippet("c", "Else", "Email")],
                      {"b": {"count": 999999, "last": 1000}, "c": {"count": 999999, "last": 1000}})
        self.assertEqual([r["id"] for r in index.search("email", now=1000)], ["a", "b", "c"])

    def test_usage_frequency_decay_and_stability(self):
        stats = {"b": {"count": 10, "last": 10000000}, "a": {"count": 1000000, "last": 0}}
        index = Index([snippet("a"), snippet("b")], stats)
        self.assertEqual(index.search("email", now=10000000)[0]["id"], "b")
        stats["a"] = {"count": 20, "last": 10000000}
        self.assertEqual(index.search("email", now=10000000)[0]["id"], "a")
        self.assertEqual(index.search("eml"), index.search("eml"))

    def test_groups_filters_unicode_and_fuzzy(self):
        index = Index(synthetic_records(synthetic_library(10), 100))
        rows = index.search("")
        self.assertEqual(rows[0]["id"], "clipboard-00099")
        self.assertTrue(any(r["kind"] == "snippet" for r in rows))
        self.assertTrue(all(r["kind"] == "snippet" for r in index.search("", "snippet")))
        self.assertEqual(index.search("!eml")[0]["id"], "snippet-0000")
        self.assertEqual(index.search("καλη")[0]["id"], "snippet-0001")

    def test_index_bounds_payload_and_search_has_no_io(self):
        index = Index([snippet("a", body="x" * 100000 + "hidden")])
        self.assertEqual(index.search("hidden"), [])
        from unittest.mock import patch
        with patch("builtins.open", side_effect=AssertionError("disk access")), patch("subprocess.Popen", side_effect=AssertionError("process")):
            self.assertEqual(index.search("xxx")[0]["id"], "a")


class TemplateTests(unittest.TestCase):
    def test_once_escapes_unknown_and_local_time(self):
        result = render("{date} {time} {clipboard} {unknown} {{date}}", "{time}", datetime(2026, 10, 15, 9, 7))
        self.assertEqual(result.text, "2026-10-15 09:07 {time} {unknown} {date}")

    def test_unicode_multiline_cursor(self):
        result = render("🌍\n{clipboard}{cursor}end", "é\n")
        self.assertEqual(result.text, "🌍\né\nend")
        self.assertEqual(result.cursor, 4)
        self.assertEqual(utf16_length(result.text[:result.cursor]), 5)
        with self.assertRaises(ValueError):
            render("{cursor}{cursor}")

    def test_bounds(self):
        with self.assertRaises(ValueError):
            render("{clipboard}", "a" * 16385)


class KeywordTests(unittest.TestCase):
    def setUp(self):
        self.snippets = synthetic_library(4)["snippets"]

    def test_conflicts_both_orders_optional_case_sensitive(self):
        for keys in (("!a", "!ab"), ("!ab", "!a"), ("!a", "!a")):
            with self.assertRaises(ValueError):
                validate_keywords([{"keyword": k} for k in keys])
        validate_keywords([{"keyword": ""}, {"keyword": ""}, {"keyword": "!a"}, {"keyword": "!A"}])

    def test_scratch_boundaries(self):
        for prefix in ("!email", " !email", "(!email", "\n!email"):
            self.assertEqual(scratch_match(prefix, self.snippets)["id"], "snippet-0000")
        for prefix in ("a!email", "_!email", "é!email", "\u0301!email", "!Email", "!emai"):
            self.assertIsNone(scratch_match(prefix, self.snippets))

    def test_matcher_gates_context_switch_and_injection(self):
        matcher = Matcher(self.snippets)
        self.assertIsNone(matcher.feed("!email", "scratch/us", allowed=True, boundary=True))
        matcher.enabled = True
        self.assertIsNone(matcher.feed("!em", "scratch/dvorak", allowed=True, boundary=True))
        self.assertIsNone(matcher.feed("ail", "scratch/qwerty", allowed=True))
        self.assertEqual(matcher.buffer, "")
        self.assertIsNone(matcher.feed("!email", "scratch/us", allowed=True, injected=True, boundary=True))
        self.assertIsNone(matcher.context)
        self.assertEqual(matcher.feed("!email", "scratch/us", allowed=True, boundary=True), "snippet-0000")
        for allowed, context in ((False, "password-manager"), (False, "locked"), (False, "picker"), (True, None)):
            self.assertIsNone(matcher.feed("!email", context, allowed=allowed, boundary=True))
            self.assertEqual(matcher.buffer, "")

    def test_ephemeral_bound(self):
        matcher = Matcher(self.snippets)
        matcher.enabled = True
        matcher.feed("x" * 100000, "scratch", allowed=True, boundary=True)
        self.assertLessEqual(len(matcher.buffer), 64)
        matcher.reset()
        self.assertIsNone(matcher.context)


if __name__ == "__main__":
    unittest.main()
