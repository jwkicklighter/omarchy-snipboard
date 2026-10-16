import copy
import json
import os
from pathlib import Path
import tempfile
import unittest
from unittest.mock import patch

from snipboard.core import Index, synthetic_library
from snipboard.storage import Store, atomic_json, read_json, validate_library, validate_usage


class StorageTests(unittest.TestCase):
    def setUp(self):
        self.directory = tempfile.TemporaryDirectory(prefix="snipboard-", dir="/tmp/opencode")
        self.addCleanup(self.directory.cleanup)
        self.root = Path(self.directory.name)
        self.store = Store(self.root)
        self.library = self.store.initialize(synthetic_library(4))

    def test_roundtrip_exact_body_stable_id_and_usage(self):
        fresh = copy.deepcopy(self.library)
        fresh["snippets"][0]["body"] = "\t🌍\n trailing \n"
        fresh["snippets"][0]["name"] = "Renamed"
        self.store.save(fresh)
        self.assertEqual(Store(self.root).reload(), fresh)
        usage = {"snippet-0000": {"count": 9, "last": 1000}}
        atomic_json(self.store.usage_path, usage, validate_usage)
        self.assertEqual(read_json(self.store.usage_path, validate_usage), usage)
        self.assertNotIn("usage", self.store.library)
        self.assertEqual(self.store.library_path.stat().st_mode & 0o777, 0o600)

    def test_invalid_missing_preserve_last_good_and_refuse_writes(self):
        self.store.library_path.write_text("broken")
        with self.assertRaises(ValueError):
            self.store.reload()
        self.assertEqual(self.store.library, self.library)
        self.assertTrue(self.store.readonly)
        with self.assertRaises(ValueError):
            self.store.save(self.library)
        self.assertEqual(self.store.library_path.read_text(), "broken")
        self.store.library_path.unlink()
        with self.assertRaises(OSError):
            Store(self.root).initialize(synthetic_library())
        self.assertFalse(self.store.library_path.exists())

    def test_atomic_failure_retains_original_and_cleans_temp(self):
        original = self.store.library_path.read_bytes()
        with patch("os.replace", side_effect=OSError("synthetic failure")):
            with self.assertRaises(OSError):
                self.store.save(self.library)
        self.assertEqual(self.store.library_path.read_bytes(), original)
        self.assertEqual(list(self.root.glob(".snipboard-*")), [])

    def test_schema_duplicate_ids_keywords_private_fields_and_versions(self):
        mutations = [lambda x: x.update(schemaVersion=True),
                     lambda x: x.update(history=[]),
                     lambda x: x["snippets"].append(x["snippets"][0]),
                     lambda x: x["snippets"][1].update(keyword="!email-long"),
                     lambda x: x["portable"].update(machinePath="/private")]
        for mutate in mutations:
            invalid = copy.deepcopy(self.library)
            mutate(invalid)
            with self.assertRaises(ValueError):
                validate_library(invalid)

    def test_symlink_permissions_duplicate_json_keys(self):
        link = self.root / "link"
        link.symlink_to(self.store.library_path)
        with self.assertRaises(OSError):
            read_json(link, validate_library)
        with self.assertRaises(ValueError):
            atomic_json(link, self.library, validate_library)
        os.chmod(self.store.library_path, 0o644)
        with self.assertRaises(ValueError):
            read_json(self.store.library_path, validate_library)
        os.chmod(self.store.library_path, 0o600)
        self.store.library_path.write_text('{"schemaVersion":1,"schemaVersion":1}')
        with self.assertRaises(ValueError):
            read_json(self.store.library_path, validate_library)

    def test_external_change_is_not_silently_overwritten(self):
        external = copy.deepcopy(self.library)
        external["snippets"][0]["body"] = "External scratch change"
        atomic_json(self.store.library_path, external, validate_library)
        with self.assertRaises(ValueError):
            self.store.save(self.library)
        self.assertTrue(self.store.readonly)
        self.assertEqual(read_json(self.store.library_path, validate_library), external)

    def test_missing_or_invalid_storage_on_save_becomes_readonly(self):
        self.store.library_path.unlink()
        with self.assertRaises(OSError):
            self.store.save(self.library)
        self.assertTrue(self.store.readonly)
        self.assertEqual(self.store.library, self.library)
        self.assertFalse(self.store.library_path.exists())

    def test_template_validation_and_revision(self):
        revision = self.store.revision
        fresh = copy.deepcopy(self.library)
        fresh["snippets"][0]["body"] = "{{cursor}} {cursor}"
        self.store.save(fresh)
        self.assertNotEqual(revision, self.store.revision)
        self.assertEqual(Store(self.root).reload(), fresh)
        for field, value in (("body", "{cursor}{cursor}"), ("name", " \t\n"), ("body", "\ud800")):
            invalid = copy.deepcopy(fresh)
            invalid["snippets"][0][field] = value
            with self.assertRaises(ValueError):
                self.store.save(invalid)
        self.assertEqual(self.store.library, fresh)


if __name__ == "__main__":
    unittest.main()
