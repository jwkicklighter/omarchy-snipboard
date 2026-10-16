import random
import unittest

from snipboard.core import Index, synthetic_library, synthetic_records
from snipboard.native import NativeIndex


class NativeTests(unittest.TestCase):
    def test_reference_equivalence(self):
        records = synthetic_records(synthetic_library(30), 200)
        records.extend([dict(id="unicode-case", kind="snippet", name="Straße", body="İstanbul Σ 🌍", keyword="", notes=""),
                        dict(id="unicode-bound", kind="snippet", name="Bound", body="🌍" * 510 + "END", keyword="", notes="")])
        usage = {"snippet-0004": {"count": 12, "last": 100000}}
        reference, native = Index(records, usage), NativeIndex(records, usage)
        self.addCleanup(native.close)
        random.seed(42)
        queries = ["", "email", "!eml", "project", "river", "καλη", "🌍", "zzzz", "STRASSE", "i̇st", "Σ", "EN"]
        queries += ["".join(random.choices("project amber 123", k=random.randrange(1, 8))) for _ in range(100)]
        for kind in ("all", "snippet", "clipboard"):
            for query in queries:
                self.assertEqual([r["id"] for r in native.search(query, kind, now=100000)],
                                 [r["id"] for r in reference.search(query, kind, now=100000)], (query, kind))
        usage["snippet-0005"] = {"count": 99, "last": 100000}
        native.update_usage()
        self.assertEqual(native.search("project", now=100000), reference.search("project", now=100000))
