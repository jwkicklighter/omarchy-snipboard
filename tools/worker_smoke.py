#!/usr/bin/env python3
"""Exercise the worker wire handshake without clipboard access or real data."""

import json
from pathlib import Path
import select
import subprocess
import sys
import tempfile


def smoke(worker):
    with tempfile.TemporaryDirectory(prefix="snipboard-worker-smoke-") as data:
        process = subprocess.Popen(
            [str(Path(worker).resolve()), "--data", data],
            stdin=subprocess.PIPE, stdout=subprocess.PIPE, stderr=subprocess.PIPE,
            text=True,
        )
        def read():
            if not select.select([process.stdout], [], [], 15)[0]:
                raise ValueError("Worker protocol timed out")
            response = json.loads(process.stdout.readline())
            if "error" in response:
                raise ValueError(f"Worker rejected smoke operation: {response.get('op')}")
            return response

        def request(op, **fields):
            process.stdin.write(json.dumps(dict(op=op, **fields)) + "\n")
            process.stdin.flush()
            return read()

        def require(condition):
            if not condition:
                raise ValueError("Published worker is incompatible with plugin protocol expectations")

        try:
            require(read().get("op") == "ready")
            # These are the UI's wire operations, not OS clipboard/paste actions.
            require(request("search", query="", kind="all")["rows"] == [])
            text = "Disposable fixture\nΚαλημέρα 🌍 {date}"
            captured = request("clipboard-capture", text=text)["record"]
            require(request("preview", record=captured)["body"] == text)
            require(request("render", record=captured)["text"] == text)
            request("clipboard-delete", record=captured)
            require(request("search", query="", kind="clipboard")["rows"] == [])
            draft = request("snippet-get")
            fields = dict(name="Release fixture", body="Exact\r\nUnicode 🌍", keyword="!releasefixture", notes="")
            saved = request("snippet-save", record="", revision=draft["revision"], fields=fields)
            require(request("preview", record=saved["record"])["body"] == fields["body"])
            require(request("snippet-delete", record=saved["record"], revision=saved["revision"])["deleted"])
            require(request("search", query="", kind="all")["rows"] == [])
            process.stdin.close()
            require(process.wait(timeout=15) == 0)
        finally:
            if process.poll() is None:
                process.kill()
                process.wait()
            process.stdin.close()
            process.stdout.close()
            process.stderr.close()


if __name__ == "__main__":
    smoke(sys.argv[1])
