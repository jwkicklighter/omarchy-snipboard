"""Release automation tests do not contact GitHub or read real clipboard data."""

import importlib.util
import json
import os
from pathlib import Path
import shutil
import subprocess
import sys
import tempfile
import unittest
from unittest.mock import patch

ROOT = Path(__file__).resolve().parent.parent


def load_tool(name):
    spec = importlib.util.spec_from_file_location(name, ROOT / "tools" / f"{name}.py")
    module = importlib.util.module_from_spec(spec)
    sys.modules[name] = module
    spec.loader.exec_module(module)
    return module


package = load_tool("package")
marketplace = load_tool("marketplace")
bundle = load_tool("bundle")


class ReleaseTests(unittest.TestCase):
    def test_release_please_configuration(self):
        config = json.loads((ROOT / "release-please-config.json").read_text())
        self.assertEqual(config["release-type"], "rust")
        self.assertFalse(config["include-component-in-tag"])
        self.assertTrue(config["draft"])
        self.assertTrue(config["force-tag-creation"])
        self.assertEqual(config["packages"]["."]["extra-files"], [
            {"type": "json", "path": "manifest.json", "jsonpath": "$.version"},
        ])
        # This is the last released version, not necessarily the pending version.
        manifest = json.loads((ROOT / ".release-please-manifest.json").read_text())
        self.assertRegex(manifest["."], r"^\d+\.\d+\.\d+$")

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
        self.assertIn("bin/linux-x86_64/snipboard-worker", paths)
        self.assertIn("bin/linux-x86_64/worker.json", paths)
        self.assertFalse(any(path.startswith("build/") for path in paths))
        self.assertIn("SbActions.js", paths)
        self.assertIn("preview.png", paths)
        self.assertFalse(any(".git" in path or "local.json" in path or path.startswith("tests/") for path in paths))

    def test_marketplace_body(self):
        sha = "a" * 40
        body = marketplace.verification_body(sha, "v0.1.0")
        self.assertIn(f"### Target commit\n\n{sha}", body)
        self.assertIn("Verify and publish a newer upstream commit", body)
        self.assertIn(f"<!-- snipboard-release:{sha} -->", body)
        self.assertIn("needs no Rust toolchain", body)
        self.assertNotIn("Manual setup remains required", body)
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


class BundledWorkerTests(unittest.TestCase):
    def setUp(self):
        self.directory = tempfile.TemporaryDirectory()
        self.addCleanup(self.directory.cleanup)
        self.root = Path(self.directory.name)
        for name in ("Cargo.toml", "Cargo.lock", "manifest.json"):
            shutil.copyfile(ROOT / name, self.root / name)
        for name in ("src", ".cargo", "bin"):
            shutil.copytree(ROOT / name, self.root / name)

    def test_shipped_worker_and_qml_path(self):
        bundle.check_bundle(self.root)
        qml = (ROOT / "Snipboard.qml").read_text()
        self.assertIn('/bin/linux-x86_64/snipboard-worker', qml)
        self.assertNotIn('/build/release/snipboard-worker', qml)
        self.assertEqual(qml.count('root.workerExecutable'), 2)

    def test_worker_corruption_is_rejected(self):
        with (self.root / bundle.WORKER).open("ab") as worker:
            worker.write(b"corrupt")
        with self.assertRaisesRegex(ValueError, "stale or corrupt"):
            bundle.check_bundle(self.root)

    def test_changed_rust_and_included_lua_are_rejected(self):
        for name in ("src/core.rs", "src/paste.lua", "Cargo.lock", ".cargo/config.toml"):
            with self.subTest(name=name):
                path = self.root / name
                original = path.read_bytes()
                path.write_bytes(original + b"\n")
                with self.assertRaisesRegex(ValueError, "stale or corrupt"):
                    bundle.check_bundle(self.root)
                path.write_bytes(original)

    def test_version_mismatch_is_rejected(self):
        path = self.root / bundle.METADATA
        metadata = json.loads(path.read_text())
        metadata["version"] = "999.0.0"
        path.write_text(json.dumps(metadata))
        with self.assertRaisesRegex(ValueError, "stale or corrupt"):
            bundle.check_bundle(self.root)

    def test_missing_executable_bit_is_rejected(self):
        (self.root / bundle.WORKER).chmod(0o644)
        with self.assertRaisesRegex(ValueError, "regular executable"):
            bundle.check_bundle(self.root)

    def test_wrong_architecture_and_symlink_are_rejected(self):
        worker = self.root / bundle.WORKER
        worker.write_bytes(b"not an ELF binary")
        with self.assertRaisesRegex(ValueError, "x86_64 ELF"):
            bundle.check_executable(worker)
        worker.unlink()
        worker.symlink_to(ROOT / bundle.WORKER)
        with self.assertRaisesRegex(ValueError, "regular executable"):
            bundle.check_executable(worker)

    def test_newer_glibc_is_rejected(self):
        with patch.object(bundle.subprocess, "run", return_value=subprocess.CompletedProcess([], 0, "GLIBC_2.40")):
            with self.assertRaisesRegex(ValueError, "glibc <= 2.39"):
                bundle.check_executable(self.root / bundle.WORKER)

    def test_refresh_writes_a_valid_executable_bundle(self):
        old_inode = (self.root / bundle.WORKER).stat().st_ino
        (self.root / bundle.METADATA).unlink()
        bundle.write_bundle(ROOT / bundle.WORKER, self.root)
        self.assertTrue(os.access(self.root / bundle.WORKER, os.X_OK))
        self.assertNotEqual((self.root / bundle.WORKER).stat().st_ino, old_inode)
        self.assertFalse(list((self.root / bundle.DIRECTORY).glob(".worker-*")))
        self.assertEqual(bundle.check_bundle(self.root)["version"], package.check_versions())

    def test_fresh_install_runs_without_cargo_or_build_directory(self):
        self.assertFalse((self.root / "build").exists())
        with tempfile.TemporaryDirectory() as data:
            process = subprocess.run(
                [str(self.root / bundle.WORKER), "--data", data],
                input="", text=True, capture_output=True, timeout=15, check=True,
                env={"PATH": "", "HOME": data},
            )
        self.assertEqual(json.loads(process.stdout.splitlines()[0])["op"], "ready")

    def test_bundled_clipboard_callback_needs_no_build_directory(self):
        text = "Disposable fixture\nΚαλημέρα 🌍"
        process = subprocess.run(
            [str(self.root / bundle.WORKER), "--clipboard-event"],
            input=text, text=True, capture_output=True, timeout=15, check=True,
            env={"PATH": "", "CLIPBOARD_STATE": "data"},
        )
        self.assertEqual(json.loads(process.stdout), {"text": text})

    def test_docs_do_not_require_manual_build_for_install(self):
        readme = (ROOT / "README.md").read_text()
        self.assertIn("omarchy plugin add https://github.com/jwkicklighter/omarchy-snipboard --enable", readme)
        self.assertNotIn("Manual build required", readme)
