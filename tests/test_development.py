from pathlib import Path
import tempfile
import unittest

from tools.development import relocate, unlink_work


class DevelopmentTests(unittest.TestCase):
    def test_one_tree_and_no_overwrite_or_delete(self):
        with tempfile.TemporaryDirectory(dir="/tmp/opencode") as directory:
            root = Path(directory)
            source, destination = root / "work", root / "plugins/id"
            (source / ".git").mkdir(parents=True)
            (source / "user-work").write_text("preserved")
            relocate(source, destination, source)
            self.assertTrue(source.is_symlink())
            self.assertEqual((source / "user-work").read_text(), "preserved")
            with self.assertRaises(ValueError):
                relocate(destination, destination, source)
            with self.assertRaises(ValueError):
                unlink_work(destination, source)
            unlink_work(source, destination)
            self.assertFalse(source.exists())
            self.assertEqual((destination / "user-work").read_text(), "preserved")
