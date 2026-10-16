"""Offscreen editor tests with inert theme tokens; never loads the shell/plugin."""
import os
from pathlib import Path
import shutil
import subprocess
import tempfile
import unittest


class QmlTests(unittest.TestCase):
    def test_editor_offscreen(self):
        runner = shutil.which("qmltestrunner") or "/usr/lib/qt6/bin/qmltestrunner"
        if not Path(runner).is_file():
            self.skipTest("Qt Quick Test runner unavailable")
        source = Path(__file__).resolve().parent / "qml"
        with tempfile.TemporaryDirectory(prefix="snipboard-qml-", dir="/tmp/opencode") as directory:
            env = dict(os.environ, QT_QPA_PLATFORM="offscreen", QT_QUICK_BACKEND="software",
                       QML_DISABLE_DISK_CACHE="1", XDG_CACHE_HOME=directory)
            result = subprocess.run([runner, "-input", str(source / "tst_editor.qml"),
                                     "-import", str(source / "imports")],
                                    env=env, capture_output=True, text=True, timeout=45)
            self.assertEqual(result.returncode, 0, result.stdout + result.stderr)
