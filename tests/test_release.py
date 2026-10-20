"""Release automation tests do not contact GitHub or read real clipboard data."""

import importlib.util
import json
from pathlib import Path
import tempfile
import unittest
from unittest.mock import patch

ROOT = Path(__file__).resolve().parent.parent


def load_tool(name):
    spec = importlib.util.spec_from_file_location(name, ROOT / "tools" / f"{name}.py")
    module = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(module)
    return module


package = load_tool("package")
marketplace = load_tool("marketplace")


class ReleaseTests(unittest.TestCase):
    def test_versions_match(self):
        version = package.check_versions()
        self.assertEqual(version, package.check_versions(tag=f"v{version}"))
        with self.assertRaises(ValueError):
            package.check_versions(tag="v999.0.0")

    def test_mismatched_manifest_is_rejected(self):
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            for name in ["manifest.json", "Cargo.toml", "Cargo.lock"]:
                (root / name).write_bytes((ROOT / name).read_bytes())
            manifest = json.loads((root / "manifest.json").read_text())
            manifest["version"] = "999.0.0"
            (root / "manifest.json").write_text(json.dumps(manifest))
            with self.assertRaises(ValueError):
                package.check_versions(root)

    def test_package_allowlist(self):
        paths = [path.relative_to(ROOT).as_posix() for path in package.release_files()]
        self.assertIn("build/release/snipboard-worker", paths)
        self.assertIn("SbActions.js", paths)
        self.assertIn("preview.png", paths)
        self.assertFalse(any(".git" in path or "local.json" in path or path.startswith("tests/") for path in paths))

    def test_marketplace_body(self):
        sha = "a" * 40
        body = marketplace.verification_body(sha, "v0.1.0")
        self.assertIn(f"### Target commit\n\n{sha}", body)
        self.assertIn("Verify and publish a newer upstream commit", body)
        self.assertIn(f"<!-- snipboard-release:{sha} -->", body)
        self.assertIn("Manual setup remains required", body)
        for commit, tag in [("abc", "v0.1.0"), (sha, "--help")]:
            with self.assertRaises(ValueError):
                marketplace.verification_body(commit, tag)

    def run_marketplace(self, head, issues):
        sha = "a" * 40
        responses = [
            json.dumps({"draft": False, "prerelease": False}),
            sha, "master", head, json.dumps([issues]), "https://example.test/issue/1",
        ]
        with patch.object(marketplace, "gh", side_effect=responses) as gh:
            with patch("sys.argv", ["marketplace.py", "--tag", "v0.1.0"]):
                marketplace.main()
            return gh

    def test_marketplace_creates_exact_commit_request(self):
        gh = self.run_marketplace("a" * 40, [])
        self.assertEqual(gh.call_args.args[:2], ("issue", "create"))
        self.assertIn("a" * 40, gh.call_args.kwargs["body"])

    def test_marketplace_retry_does_not_duplicate_closed_request(self):
        issue = {"body": f"<!-- snipboard-release:{'a' * 40} -->", "html_url": "https://example.test/existing"}
        gh = self.run_marketplace("a" * 40, [issue])
        self.assertFalse(any(call.args[:2] == ("issue", "create") for call in gh.call_args_list))

    def test_marketplace_rejects_stale_tag_before_writing(self):
        with self.assertRaisesRegex(ValueError, "stale"):
            self.run_marketplace("b" * 40, [])
