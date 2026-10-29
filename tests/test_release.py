"""Release automation tests do not contact GitHub or read real clipboard data."""

import importlib.util
from concurrent.futures import ThreadPoolExecutor
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
runtime = load_tool("worker_runtime")
engine = load_tool("engine_release")
split = load_tool("check_pr_split")


class ReleaseTests(unittest.TestCase):
    def test_release_please_configuration(self):
        config = json.loads((ROOT / "release-please-config.json").read_text())
        self.assertEqual(config["release-type"], "simple")
        self.assertEqual(config["packages"]["."]["version-file"], ".plugin-version")
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

    def test_plugin_version_is_independent_of_engine(self):
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            for name in ["manifest.json", ".plugin-version", "Cargo.toml", "Cargo.lock"]:
                (root / name).write_bytes((ROOT / name).read_bytes())
            manifest = json.loads((root / "manifest.json").read_text())
            manifest["version"] = "999.0.0"
            (root / "manifest.json").write_text(json.dumps(manifest))
            (root / ".plugin-version").write_text("999.0.0\n")
            self.assertEqual(package.check_versions(root), "999.0.0")

    def test_package_allowlist(self):
        paths = [path.relative_to(ROOT).as_posix() for path in package.release_files()]
        self.assertIn("engine/release.pin.json", paths)
        self.assertIn("tools/worker_runtime.py", paths)
        self.assertFalse(any(path.startswith("bin/") for path in paths))
        self.assertFalse(any(path.startswith("build/") for path in paths))
        self.assertIn("SbActions.js", paths)
        self.assertIn("SbPreview.qml", paths)
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


class PinnedWorkerTests(unittest.TestCase):
    def setUp(self):
        self.directory = tempfile.TemporaryDirectory()
        self.addCleanup(self.directory.cleanup)
        self.root = Path(self.directory.name)
        self.asset = self.root / "candidate"
        self.asset.write_bytes(b"\x7fELF\x02\x01" + b"\0" * 12 + b"\x3e\0" + b"fixture")
        self.pin = {"repository": runtime.REPOSITORY, "version": "1.2.3", "tag": "engine-1.2.3", "asset": runtime.ASSET, "sha256": runtime.digest(self.asset)}
        self.fetches = []

    def fetch(self, url, destination):
        self.fetches.append(url)
        shutil.copyfile(self.asset, destination)

    def ensure(self):
        return runtime.ensure_worker(self.pin, self.root / "data", self.fetch)

    def test_install_then_offline_cache_hit(self):
        worker = self.ensure()
        self.assertTrue(os.access(worker, os.X_OK))
        self.assertEqual(runtime.digest(worker), self.pin["sha256"])
        self.assertEqual(self.ensure(), worker)
        self.assertEqual(len(self.fetches), 1)
        self.assertIn("/engine-1.2.3/", self.fetches[0])
        self.assertNotIn("latest", self.fetches[0])
        self.assertFalse(list(worker.parent.glob(".install-*")))

    def test_concurrent_installers_download_once(self):
        with ThreadPoolExecutor(max_workers=4) as pool:
            workers = list(pool.map(lambda _: self.ensure(), range(8)))
        self.assertEqual(len(set(workers)), 1)
        self.assertEqual(len(self.fetches), 1)

    def test_interrupted_download_never_installs_partial_worker(self):
        def partial(url, path):
            path.write_bytes(b"partial")
            raise OSError("connection closed")
        with self.assertRaises(OSError):
            runtime.ensure_worker(self.pin, self.root / "data", partial)
        self.assertFalse(list((self.root / "data").rglob("snipboard-worker")))
        self.ensure()

    def test_unsupported_platform_does_not_download(self):
        with patch.object(runtime.platform, "machine", return_value="aarch64"):
            with self.assertRaisesRegex(ValueError, "x86_64 only"):
                self.ensure()
        self.assertEqual(self.fetches, [])

    def test_corrupt_cache_and_missing_executable_bit_are_repaired(self):
        worker = self.ensure()
        worker.write_bytes(b"corrupt")
        self.ensure()
        self.assertEqual(runtime.digest(worker), self.pin["sha256"])
        worker.chmod(0o600)
        self.ensure()
        self.assertTrue(os.access(worker, os.X_OK))
        self.assertEqual(len(self.fetches), 3)

    def test_mismatching_download_does_not_replace_existing_worker(self):
        worker = self.ensure()
        worker.write_bytes(b"previous")
        self.asset.write_bytes(b"tampered")
        with self.assertRaisesRegex(ValueError, "checksum mismatch"):
            self.ensure()
        self.assertEqual(worker.read_bytes(), b"previous")
        self.assertFalse(list(worker.parent.glob(".install-*")))

    def test_network_failure_installs_nothing(self):
        with self.assertRaises(OSError):
            runtime.ensure_worker(self.pin, self.root / "data", lambda *_: (_ for _ in ()).throw(OSError("offline")))
        self.assertFalse(list((self.root / "data").rglob("snipboard-worker")))

    def test_new_pin_installs_separately_and_rollback_is_offline(self):
        old_pin = dict(self.pin)
        old_worker = self.ensure()
        self.asset.write_bytes(self.asset.read_bytes() + b"v2")
        self.pin.update(version="1.2.4", tag="engine-1.2.4", sha256=runtime.digest(self.asset))
        new_worker = self.ensure()
        self.assertNotEqual(new_worker, old_worker)
        self.pin = old_pin
        self.assertEqual(self.ensure(), old_worker)
        self.assertEqual(len(self.fetches), 2)

    def test_wrong_architecture_is_rejected_even_with_matching_hash(self):
        self.asset.write_bytes(b"not ELF")
        self.pin["sha256"] = runtime.digest(self.asset)
        with self.assertRaisesRegex(ValueError, "x86_64 ELF"):
            self.ensure()

    def test_pin_schema_rejects_unsafe_names_and_unknown_fields(self):
        path = self.root / "pin.json"
        for key, value in [("tag", "../../unsafe"), ("asset", "../../worker"), ("repository", "other/repo"), ("sha256", "no"), ("unexpected", True)]:
            with self.subTest(key=key):
                path.write_text(json.dumps(dict(self.pin, **{key: value})))
                with self.assertRaises(ValueError):
                    runtime.read_pin(path)
        path.write_text(json.dumps(self.pin))
        self.assertEqual(runtime.read_pin(path), self.pin)

    def test_shipped_pin_and_qml_bootstrap(self):
        runtime.read_pin(ROOT / "engine/release.pin.json")
        qml = (ROOT / "Snipboard.qml").read_text()
        self.assertIn('/tools/worker_runtime.py', qml)
        self.assertNotIn('/bin/linux-x86_64/', qml)
        self.assertNotIn('/build/release/snipboard-worker', qml)
        self.assertEqual(qml.count('root.workerExecutable'), 4)

    def test_split_ownership_allows_pin_with_ui_not_engine(self):
        self.assertEqual(split.groups(["src/main.rs", "Cargo.lock"])[1], [])
        self.assertEqual(split.groups(["Snipboard.qml", "engine/release.pin.json"])[0], [])
        self.assertEqual(split.groups(["tests/qml.rs", "tests/qml/tst_editor.qml"])[0], [])
        engine_paths, ui = split.groups(["src/core.rs", "Snipboard.qml"])
        self.assertTrue(engine_paths and ui)

    def test_engine_version_does_not_depend_on_ui(self):
        for name in ("Cargo.toml", "Cargo.lock"):
            shutil.copyfile(ROOT / name, self.root / name)
        self.assertRegex(engine.engine_version(self.root), r"^\d+\.\d+\.\d+$")

    def test_engine_package_and_glibc_floor(self):
        self.asset.chmod(0o755)
        with patch.object(engine.subprocess, "check_output", return_value="GLIBC_2.40"):
            with self.assertRaisesRegex(ValueError, "glibc <= 2.39"):
                engine.check_executable(self.asset)
        version = engine.engine_version()
        with patch.object(engine, "check_executable"):
            engine.package_engine(self.asset, self.root / "dist", f"engine-{version}")
        pin = runtime.read_pin(self.root / "dist/release.pin.json")
        self.assertEqual(pin["sha256"], runtime.digest(self.asset))
        metadata = json.loads((self.root / "dist/worker.json").read_text())
        self.assertEqual(metadata["version"], version)
        self.assertRegex(metadata["commit"], r"^[a-f0-9]{40}$")
        with self.assertRaisesRegex(ValueError, "Expected engine-"):
            engine.package_engine(self.asset, self.root / "dist", "engine-999.0.0")

    def test_pin_command_verifies_before_replacing(self):
        destination = self.root / "tracked-pin"
        destination.write_text("unchanged")
        release = json.dumps({"isDraft": False, "isPrerelease": False, "tagName": self.pin["tag"]})
        metadata = dict(version="1.2.3", target="x86_64-unknown-linux-gnu", sha256=self.pin["sha256"], commit="a" * 40)
        files = {runtime.ASSET: self.asset.read_bytes(), "release.pin.json": json.dumps(self.pin).encode(), "worker.json": json.dumps(metadata).encode()}
        import hashlib
        files["SHA256SUMS"] = "".join(f"{hashlib.sha256(files[name]).hexdigest()}  {name}\n" for name in (runtime.ASSET, "worker.json", "release.pin.json")).encode()

        def fetch(url, path):
            path.write_bytes(files[url.rsplit("/", 1)[1]])

        with patch.object(engine, "gh", side_effect=[release, "a" * 40]), patch.object(engine, "download", side_effect=fetch), patch.object(engine, "check_executable"):
            engine.pin_release(self.pin["tag"], destination)
        self.assertEqual(json.loads(destination.read_text()), self.pin)
        destination.write_text("unchanged")
        files[runtime.ASSET] = b"corrupt"
        with patch.object(engine, "gh", side_effect=[release, "a" * 40]), patch.object(engine, "download", side_effect=fetch):
            with self.assertRaisesRegex(ValueError, "checksums"):
                engine.pin_release(self.pin["tag"], destination)
        self.assertEqual(destination.read_text(), "unchanged")

    def test_draft_release_cannot_be_pinned(self):
        release = json.dumps({"isDraft": True, "isPrerelease": False, "tagName": self.pin["tag"]})
        with patch.object(engine, "gh", return_value=release), patch.object(engine, "download") as fetch:
            with self.assertRaisesRegex(ValueError, "published"):
                engine.pin_release(self.pin["tag"])
            fetch.assert_not_called()
