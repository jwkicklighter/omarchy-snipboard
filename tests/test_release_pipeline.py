"""Release coordination tests use mocked GitHub calls and never publish anything."""

import json
from pathlib import Path
import sys
import tempfile
import unittest
from unittest.mock import patch

sys.path.insert(0, str(Path(__file__).resolve().parent.parent / "tools"))
import release_pipeline as pipeline
import release_policy as policy


class PolicyTests(unittest.TestCase):
    def validate(self, paths, before="1.0.0", after="1.0.1", old_pin="1.0.0", pin="1.0.0"):
        policy.validate(paths, before, after, {"version": old_pin}, {"version": pin})

    def test_combined_feature_can_wait_for_pin(self):
        self.validate(["src/worker.rs", "Cargo.toml", "Cargo.lock", "Snipboard.qml"])

    def test_engine_change_without_bump_and_version_reuse_fail(self):
        for path in ["src/worker.rs", "Cargo.lock", "tests/worker.rs", ".cargo/config.toml"]:
            with self.subTest(path=path), self.assertRaisesRegex(ValueError, "new Cargo version"):
                self.validate([path], after="1.0.0")

    def test_pin_only_adoption_matches_source(self):
        self.validate(["engine/release.pin.json"], before="1.0.1", pin="1.0.1")
        for files in [["engine/release.pin.json", "Snipboard.qml"], ["engine/release.pin.json", "Cargo.toml"]]:
            with self.assertRaisesRegex(ValueError, "pin-only"):
                self.validate(files, pin="1.0.1")

    def test_pin_cannot_be_stale_or_rollback_or_replacement(self):
        for pin, old_pin in [("1.0.0", "1.0.0"), ("1.0.1", "1.0.1"), ("1.0.1", "1.0.2")]:
            with self.subTest(pin=pin, old_pin=old_pin), self.assertRaises(ValueError):
                self.validate(["engine/release.pin.json"], pin=pin, old_pin=old_pin)

    def test_release_metadata_waits_for_pin(self):
        with self.assertRaisesRegex(ValueError, "wait"):
            self.validate([".plugin-version", "manifest.json", "CHANGELOG.md"])
        self.validate([".plugin-version", "manifest.json"], pin="1.0.1")

    def test_ui_only_patch_can_keep_pin(self):
        self.validate(["Snipboard.qml"], after="1.0.0")

    def test_no_engine_downgrade_or_pin_ahead_of_source(self):
        with self.assertRaisesRegex(ValueError, "backwards"):
            self.validate([], before="1.0.1", after="1.0.0")
        with self.assertRaisesRegex(ValueError, "older than the pin"):
            self.validate([], after="1.0.1", pin="1.0.2")


class PipelineTests(unittest.TestCase):
    def test_release_state_rejects_prereleases(self):
        self.assertFalse(pipeline.published(None))
        self.assertFalse(pipeline.published({"draft": True, "prerelease": False}))
        self.assertTrue(pipeline.published({"draft": False, "prerelease": False}))
        with self.assertRaisesRegex(ValueError, "stable"):
            pipeline.published({"draft": False, "prerelease": True})

    def test_release_lookup_does_not_hide_api_failures(self):
        with patch.object(pipeline, "gh", return_value='[[{"tag_name":"engine-1.0.0"}],[]]'):
            self.assertIsNone(pipeline.release("engine-1.0.1"))
        with patch.object(pipeline, "gh", side_effect=RuntimeError("network")):
            with self.assertRaises(RuntimeError):
                pipeline.release("engine-1.0.1")

    def test_aligned_source_does_not_create_an_engine_release(self):
        with patch.object(pipeline, "ready", return_value=True), patch.object(pipeline, "api") as api:
            pipeline.route()
        api.assert_not_called()

    def route(self, refs, published=False, latest="a" * 40):
        responses = [{"default_branch": "master"}, {"sha": latest}, refs]
        if not refs:
            responses += [{"sha": "tag-object"}, {"ref": "refs/tags/engine-1.0.1"}]
        responses += [{"sha": "a" * 40}]
        with patch.object(pipeline, "ready", return_value=False), \
                patch.object(pipeline, "engine_version", return_value="1.0.1"), \
                patch.object(pipeline, "read_pin", return_value={"version": "1.0.0"}), \
                patch.object(pipeline, "api", side_effect=responses) as api, \
                patch.object(pipeline.subprocess, "check_output", return_value="a" * 40), \
                patch.object(pipeline, "source_matches") as source, \
                patch.object(pipeline, "release", return_value={"draft": not published, "prerelease": False}), \
                patch.object(pipeline, "dispatch") as dispatch:
            pipeline.route()
        return api, source, dispatch

    def test_new_version_creates_annotated_tag_and_dispatches_tagged_build(self):
        api, source, dispatch = self.route([])
        self.assertEqual(api.call_args_list[3].args, ("git/tags",))
        self.assertEqual(api.call_args_list[3].kwargs["object"], "a" * 40)
        source.assert_called_once_with("a" * 40)
        dispatch.assert_called_once_with("engine-release.yml", "engine-1.0.1")

    def test_retry_reuses_published_release_without_rebuilding(self):
        api, _, dispatch = self.route([{"ref": "refs/tags/engine-1.0.1"}], published=True)
        self.assertFalse(any(call.kwargs for call in api.call_args_list))
        dispatch.assert_called_once_with("adopt-engine.yml", "master", "engine-1.0.1")

    def test_stale_default_branch_run_does_not_tag_or_dispatch(self):
        api, source, dispatch = self.route([], latest="b" * 40)
        self.assertEqual(api.call_count, 2)
        source.assert_not_called()
        dispatch.assert_not_called()

    def test_tag_reuse_with_different_engine_source_is_rejected(self):
        with patch.object(pipeline.subprocess, "check_output", return_value="src/worker.rs\n"):
            with self.assertRaisesRegex(ValueError, "bump"):
                pipeline.source_matches("a" * 40)
        with patch.object(pipeline.subprocess, "check_output", return_value="Snipboard.qml\nengine/release.pin.json\n"):
            pipeline.source_matches("a" * 40)

    def test_superseded_engine_cannot_open_a_pin_pr(self):
        with patch.object(pipeline, "engine_version", return_value="1.0.2"), \
                patch.object(pipeline, "pin_release") as pin, patch.object(pipeline, "output") as output:
            pipeline.prepare_pin("engine-1.0.1")
        pin.assert_not_called()
        output.assert_called_once_with("adopt", False)

    def test_pin_preparation_checks_provenance_and_limits_changed_paths(self):
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            (root / "engine").mkdir()
            pin = root / "engine/release.pin.json"
            for paths, fails in [("engine/release.pin.json\n", False), ("engine/release.pin.json\nSnipboard.qml\n", True)]:
                pin.write_text("old")
                with patch.object(pipeline, "ROOT", root), \
                        patch.object(pipeline, "engine_version", return_value="1.0.1"), \
                        patch.object(pipeline, "api", return_value={"sha": "a" * 40}), \
                        patch.object(pipeline, "source_matches"), \
                        patch.object(pipeline, "pin_release", side_effect=lambda _: pin.write_text("verified")) as verify, \
                        patch.object(pipeline.subprocess, "check_output", return_value=paths), \
                        patch.object(pipeline, "output") as output:
                    if fails:
                        with self.assertRaisesRegex(ValueError, "only"):
                            pipeline.prepare_pin("engine-1.0.1")
                    else:
                        pipeline.prepare_pin("engine-1.0.1")
                        output.assert_called_once_with("adopt", True)
                    verify.assert_called_once_with("engine-1.0.1")

    def test_auto_merge_requires_strict_checks_and_admin_enforcement(self):
        good = {"required_status_checks": {"strict": True, "contexts": sorted(pipeline.REQUIRED_CHECKS)}, "enforce_admins": {"enabled": True}}
        for value in [{}, dict(good, required_status_checks={"strict": False}), dict(good, enforce_admins={"enabled": False})]:
            with patch.object(pipeline, "api", return_value=value), self.assertRaises(ValueError):
                pipeline.require_merge_policy("master")
        with patch.object(pipeline, "api", return_value=good):
            pipeline.require_merge_policy("master")

    def test_auto_merge_validates_pr_scope_and_locks_tested_head(self):
        pr = {"headRefName": "automation/engine-1.0.1", "headRefOid": "a" * 40,
              "baseRefName": "master", "isCrossRepository": False, "state": "OPEN",
              "files": [{"path": "engine/release.pin.json"}]}
        with patch.object(pipeline, "gh", side_effect=[json.dumps(pr), ""]) as gh, \
                patch.object(pipeline, "api", return_value={"default_branch": "master"}), \
                patch.object(pipeline, "require_merge_policy") as protection:
            pipeline.auto_merge(12, "pin")
        protection.assert_called_once_with("master")
        self.assertEqual(gh.call_args.args[-4:], ("--auto", "--squash", "--match-head-commit", "a" * 40))
        for changes in [{"isCrossRepository": True}, {"headRefName": "feature"}, {"files": [{"path": "Snipboard.qml"}]}]:
            with patch.object(pipeline, "gh", return_value=json.dumps(dict(pr, **changes))) as gh, \
                    patch.object(pipeline, "api", return_value={"default_branch": "master"}), self.assertRaises(ValueError):
                pipeline.auto_merge(12, "pin")
            self.assertEqual(gh.call_count, 1)

    def test_release_auto_merge_is_metadata_only(self):
        pr = {"headRefName": "release-please--branches--master", "headRefOid": "a" * 40,
              "baseRefName": "master", "isCrossRepository": False, "state": "OPEN",
              "files": [{"path": path} for path in ["CHANGELOG.md", ".plugin-version", "manifest.json", ".release-please-manifest.json"]]}
        with patch.object(pipeline, "gh", side_effect=[json.dumps(pr), ""]) as gh, \
                patch.object(pipeline, "api", return_value={"default_branch": "master"}), \
                patch.object(pipeline, "require_merge_policy"):
            pipeline.auto_merge(13, "release")
        self.assertEqual(gh.call_count, 2)
        pr["files"].append({"path": "Snipboard.qml"})
        with patch.object(pipeline, "gh", return_value=json.dumps(pr)) as gh, \
                patch.object(pipeline, "api", return_value={"default_branch": "master"}), self.assertRaisesRegex(ValueError, "Unexpected release"):
            pipeline.auto_merge(13, "release")
        self.assertEqual(gh.call_count, 1)

    def test_final_release_gate_fails_closed_during_handoff(self):
        for aligned in [False, True]:
            with patch.object(sys, "argv", ["release_pipeline.py", "ready", "--require"]), \
                    patch.object(pipeline, "ready", return_value=aligned), patch.object(pipeline, "output") as output:
                if aligned:
                    pipeline.main()
                else:
                    with self.assertRaisesRegex(ValueError, "blocked"):
                        pipeline.main()
                output.assert_called_once_with("ready", aligned)

    def test_workflow_release_gates_and_publish_order(self):
        root = Path(__file__).resolve().parent.parent
        workflow = (root / ".github/workflows/release.yml").read_text()
        self.assertIn("if: needs.gate.outputs.ready == 'true'", workflow)
        self.assertIn("release_pipeline.py ready --require", workflow)
        self.assertIn("release_pipeline.py auto-release", workflow)
        engine = (root / ".github/workflows/engine-release.yml").read_text()
        self.assertLess(engine.index("engine_release.py verify"), engine.index('gh release edit "$RELEASE_TAG" --draft=false'))
        self.assertIn("if: startsWith(github.ref, 'refs/tags/engine-')", engine)
        ci = (root / ".github/workflows/ci.yml").read_text()
        self.assertIn('SNIPBOARD_TEST_WORKER="$(python3 tools/worker_runtime.py)"', ci)
        self.assertIn("cargo test --release --locked --test worker", ci)


if __name__ == "__main__":
    unittest.main()
