import json
import os
from pathlib import Path
import signal
import subprocess
import sys
import tempfile
import unittest

from snipboard.storage import read_json, validate_usage


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
