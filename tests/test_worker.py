import json
import os
from pathlib import Path
import signal
import subprocess
import sys
import tempfile
import unittest

from snipboard.storage import atomic_json, read_json, validate_library, validate_usage


class WorkerTests(unittest.TestCase):
    def launch(self, directory):
        process = subprocess.Popen([sys.executable, "-m", "snipboard.worker", "--data", directory],
                                   stdin=subprocess.PIPE, stdout=subprocess.PIPE, stderr=subprocess.PIPE, text=True)
        self.assertEqual(json.loads(process.stdout.readline())["op"], "ready")
        def cleanup():
            if process.poll() is None:
                process.terminate()
                process.wait(timeout=5)
            process.stdin.close()
            process.stdout.close()
            process.stderr.close()
        self.addCleanup(cleanup)
        return process

    def request(self, process, **request):
        process.stdin.write(json.dumps(request) + "\n")
        process.stdin.flush()
        return json.loads(process.stdout.readline())

    def test_protocol_scratch_search_and_persistence(self):
        from snipboard.worker import Worker
        with self.assertRaises(ValueError):
            Worker(Path(__file__).resolve().parent.parent / "forbidden-data")
        with tempfile.TemporaryDirectory(dir="/tmp/opencode") as directory:
            process = self.launch(directory)
            result = self.request(process, op="search", query="email", seq=3)
            self.assertEqual(result["seq"], 3)
            self.assertEqual(result["rows"][0]["id"], "snippet-0000")
            self.assertNotIn("body", result["rows"][0])
            for keyword, text in (("!email", "scratch@example.invalid"), ("!unicode", "Hello — Καλημέρα 🌍"), ("!multi", "First line\nSecond line\nThird line")):
                expanded = self.request(process, op="scratch", prefix=keyword, enabled=True, context="trusted-scratch")
                self.assertEqual(expanded["expansion"]["text"], text)
            self.assertIsNone(self.request(process, op="scratch", prefix="!email", enabled=False, context="trusted-scratch")["expansion"])
            self.assertIsNone(self.request(process, op="scratch", prefix="!email", enabled=True, context="password-manager")["expansion"])
            self.request(process, op="used", record="snippet-0000")
            usage = read_json(Path(directory) / "usage.local.json", validate_usage)
            self.assertEqual(usage["snippet-0000"]["count"], 1)
            self.request(process, op="shutdown")
            process.wait(timeout=5)
            self.assertEqual(process.stderr.read(), "")

    def test_eof_sigterm_and_exclusive_worker(self):
        with tempfile.TemporaryDirectory(dir="/tmp/opencode") as directory:
            process = self.launch(directory)
            second = subprocess.run([sys.executable, "-m", "snipboard.worker", "--data", directory], input="", capture_output=True, text=True)
            self.assertNotEqual(second.returncode, 0)
            self.assertEqual(json.loads(second.stdout)["op"], "fatal")
            process.stdin.close()
            process.wait(timeout=5)
            self.assertEqual(process.returncode, 0)
            process = self.launch(directory)
            process.send_signal(signal.SIGTERM)
            process.wait(timeout=5)
            self.assertEqual(process.returncode, 0)

    def test_snippet_crud_exact_text_stable_usage_and_restart(self):
        with tempfile.TemporaryDirectory(dir="/tmp/opencode") as directory:
            process = self.launch(directory)
            new = self.request(process, op="snippet-get")
            self.assertEqual(new["snippet"]["id"], "")
            fields = {"name": "Fixture zephyreditor", "body": "\t🌍\n trailing \n{cursor}",
                      "keyword": "!zephyr", "notes": "searchablenote"}
            saved = self.request(process, op="snippet-save", record="", fields=fields, revision=new["revision"])
            record = saved["record"]
            self.assertNotEqual(saved["revision"], new["revision"])
            row = self.request(process, op="search", query="zephyreditor")["rows"][0]
            self.assertEqual(row["id"], record)
            self.assertEqual(self.request(process, op="preview", record=record)["body"], fields["body"])
            self.assertEqual(self.request(process, op="search", query="searchablenote")["rows"][0]["id"], record)
            self.request(process, op="used", record=record)
            fields["name"] = "Renamed zephyreditor"
            renamed = self.request(process, op="snippet-save", record=record, fields=fields, revision=saved["revision"])
            self.assertEqual(renamed["record"], record)
            usage = read_json(Path(directory) / "usage.local.json", validate_usage)
            self.assertEqual(usage[record]["count"], 1)
            self.request(process, op="shutdown")
            process.wait(timeout=5)
            process = self.launch(directory)
            edited = self.request(process, op="snippet-get", record=record)
            self.assertEqual(edited["snippet"], dict(fields, id=record))
            deleted = self.request(process, op="snippet-delete", record=record, revision=edited["revision"])
            self.assertTrue(deleted["deleted"])
            self.assertIn("error", self.request(process, op="snippet-get", record=record))
            self.assertEqual(self.request(process, op="search", query="zephyreditor")["rows"], [])
            self.assertIsNone(self.request(process, op="scratch", enabled=True, context="trusted-scratch", prefix="!zephyr")["expansion"])
            # Deleting a definition does not rewrite or export machine-local usage.
            self.assertEqual(read_json(Path(directory) / "usage.local.json", validate_usage), usage)
            self.request(process, op="shutdown")
            process.wait(timeout=5)
            process = self.launch(directory)
            self.assertIn("error", self.request(process, op="snippet-get", record=record))

    def test_editor_validation_stale_drafts_and_clipboard_are_not_mutated(self):
        with tempfile.TemporaryDirectory(dir="/tmp/opencode") as directory:
            process = self.launch(directory)
            original = self.request(process, op="snippet-get", record="snippet-0000")
            fields = {key: original["snippet"][key] for key in ("name", "body", "notes", "keyword")}
            path = Path(directory) / "library.json"
            before = path.read_bytes()
            mutations = [{"keyword": "!uni"}, {"body": "{cursor}{cursor}"}, {"name": "  "},
                         {"notes": "x" * 4097}, {"body": "x" * 65537}, {"body": "\ud800"},
                         {"keyword": "two words"}, {"keyword": "!unicode"}, {"id": "changed"}]
            for mutation in mutations:
                rejected = self.request(process, op="snippet-save", record="snippet-0000",
                                        revision=original["revision"], fields=dict(fields, **mutation))
                self.assertIn("error", rejected)
                self.assertFalse(rejected["readonly"])
                self.assertEqual(path.read_bytes(), before)
            fields["name"] = "Renamed fixture"
            saved = self.request(process, op="snippet-save", record="snippet-0000", revision=original["revision"], fields=fields)
            for operation in ("snippet-save", "snippet-delete"):
                stale = self.request(process, op=operation, record="snippet-0000", revision=original["revision"], fields=fields)
                self.assertIn("error", stale)
            for operation in ("snippet-save", "snippet-delete"):
                rejected = self.request(process, op=operation, record="clipboard-00001", revision=saved["revision"], fields=fields)
                self.assertIn("error", rejected)
            self.assertEqual(self.request(process, op="snippet-get", record="snippet-0000")["snippet"]["name"], fields["name"])

    def test_external_change_missing_and_invalid_reload_keep_last_good(self):
        with tempfile.TemporaryDirectory(dir="/tmp/opencode") as directory:
            process = self.launch(directory)
            original = self.request(process, op="snippet-get", record="snippet-0000")
            fields = {key: original["snippet"][key] for key in ("name", "body", "notes", "keyword")}
            path = Path(directory) / "library.json"
            external = read_json(path, validate_library)
            external["snippets"][0]["name"] = "External fixture"
            atomic_json(path, external, validate_library)
            failed = self.request(process, op="snippet-save", record="snippet-0000", revision=original["revision"], fields=fields)
            self.assertTrue(failed["readonly"])
            self.assertEqual(read_json(path, validate_library), external)
            self.assertEqual(self.request(process, op="snippet-get", record="snippet-0000")["snippet"], original["snippet"])
            loaded = self.request(process, op="library-reload")
            self.assertFalse(loaded["readonly"])
            self.assertEqual(self.request(process, op="search", query="External fixture")["rows"][0]["id"], "snippet-0000")
            path.write_text("invalid")
            self.assertTrue(self.request(process, op="library-reload")["readonly"])
            self.assertEqual(self.request(process, op="snippet-get", record="snippet-0000")["snippet"]["name"], "External fixture")
            path.unlink()
            self.assertIn("error", self.request(process, op="library-reload"))
            self.assertFalse(path.exists())
            atomic_json(path, external, validate_library)
            self.assertFalse(self.request(process, op="library-reload")["readonly"])

    def test_failed_persistence_keeps_live_index_and_cleans_prepared_index(self):
        from unittest.mock import patch
        from snipboard.native import NativeIndex
        from snipboard.worker import Worker
        with tempfile.TemporaryDirectory(dir="/tmp/opencode") as directory:
            worker = Worker(directory)
            self.addCleanup(worker.lock.close)
            self.addCleanup(lambda: worker.index.close())
            original = worker.index
            fields = {"name": "Unsaved fixture", "body": "exact", "notes": "", "keyword": ""}
            before = worker.store.library_path.read_bytes()
            prepared = []
            def prepare(*args):
                index = NativeIndex(*args)
                prepared.append(index)
                return index
            with patch("snipboard.worker.NativeIndex", side_effect=prepare) as factory:
                with patch("os.replace", side_effect=OSError("synthetic failure")):
                    with self.assertRaises(ValueError):
                        worker.dispatch({"op": "snippet-save", "record": "", "revision": worker.store.revision, "fields": fields})
                self.assertEqual(factory.call_count, 1)
                self.assertIsNone(prepared[0].handle)
            self.assertIs(worker.index, original)
            self.assertEqual(worker.store.library_path.read_bytes(), before)
            self.assertEqual(worker.index.search("Unsaved fixture"), [])
            self.assertEqual(list(Path(directory).glob(".snipboard-*")), [])
            self.assertTrue(worker.store.readonly)

    def test_reload_index_failure_preserves_library_and_index(self):
        from unittest.mock import patch
        from snipboard.worker import Worker
        with tempfile.TemporaryDirectory(dir="/tmp/opencode") as directory:
            worker = Worker(directory)
            self.addCleanup(worker.lock.close)
            self.addCleanup(lambda: worker.index.close())
            previous = worker.index
            revision = worker.store.revision
            external = read_json(worker.store.library_path, validate_library)
            external["snippets"][0]["name"] = "External fixture"
            atomic_json(worker.store.library_path, external, validate_library)
            with patch("snipboard.worker.NativeIndex", side_effect=OSError("synthetic failure")):
                with self.assertRaises(ValueError):
                    worker.dispatch({"op": "library-reload"})
            self.assertIs(worker.index, previous)
            self.assertEqual(worker.store.revision, revision)
            self.assertTrue(worker.store.readonly)
            loaded = worker.dispatch({"op": "library-reload"})
            self.assertNotEqual(loaded["revision"], revision)
            self.assertFalse(loaded["readonly"])

    def test_cleanup_of_owned_clipboard_process_without_touching_clipboard(self):
        from unittest.mock import Mock
        from snipboard.worker import Worker
        worker = Worker.__new__(Worker)
        worker.child = Mock()
        child = worker.child
        child.poll.return_value = None
        worker.cleanup()
        child.terminate.assert_called_once()
        child.wait.assert_called_once_with(timeout=1)
        self.assertIsNone(worker.child)
        worker.cleanup()
