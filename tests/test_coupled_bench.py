import copy
import importlib.util
import json
import os
from pathlib import Path
import shutil
import sys
import tempfile
import unittest
from unittest import mock

ROOT = Path(__file__).resolve().parents[1]
sys.path.insert(0, str(ROOT / "scripts"))
from coupled_bench import common, evidence, oracles, runner

SUITE_ROOT = ROOT / "benchmarks/coding-v1"
SUITE = common.read_json(SUITE_ROOT / "suite.json")
MATRIX = common.read_json(SUITE_ROOT / "matrix.json")


class IndependentOracles(unittest.TestCase):
    def setUp(self):
        temp = tempfile.TemporaryDirectory()
        self.addCleanup(temp.cleanup)
        self.workspace = Path(temp.name) / "workspace"
        self.case = SUITE["cases"][0]
        shutil.copytree(SUITE_ROOT / self.case["workspace"], self.workspace)

    def score(self):
        return oracles.score(self.case, SUITE_ROOT, self.workspace, Path(sys.executable))

    def test_reference_passes_but_extra_space_is_not_exact_reference(self):
        source = self.workspace / "greet.py"
        source.write_bytes(source.read_bytes().replace(b'{name}?"', b'{name}!" '))
        result = self.score()
        self.assertTrue(result["passed"])
        self.assertFalse(result["source_matches_reference"])

    def test_rewritten_candidate_tests_cannot_certify_unrepaired_source(self):
        (self.workspace / "test_greet.py").write_text("# Tests removed\n")
        result = self.score()
        self.assertFalse(result["checks"]["supplied_files_preserved"])
        self.assertFalse(result["checks"]["independent_check_0"])
        self.assertTrue(result["checks"]["original_test_count"])
        self.assertFalse(result["passed"])

    def test_passing_behavior_does_not_allow_erasing_original_context(self):
        (self.workspace / "greet.py").write_text('def greet(name):\n    return f"Hello, {name}!"\n')
        result = self.score()
        self.assertTrue(result["checks"]["independent_check_0"])
        self.assertFalse(result["checks"]["preserved_prefix"])
        self.assertFalse(result["passed"])

    @unittest.skipUnless(os.name == "posix", "symlink fixture")
    def test_candidate_symlink_is_not_followed(self):
        source = self.workspace / "greet.py"
        source.unlink()
        source.symlink_to(SUITE_ROOT / self.case["reference"] / "greet.py")
        self.assertFalse(self.score()["checks"]["candidate_artifacts_present"])

    def test_replay_oracle_rejects_nested_loss_reordering_and_duplicate_ids(self):
        case = SUITE["cases"][2]
        before = common.read_json(SUITE_ROOT / case["workspace"] / ".spiral-coder/tui_replay.json")
        after = common.read_json(SUITE_ROOT / case["reference"] / ".spiral-coder/tui_replay.json")
        self.assertTrue(oracles.preserved_json(before, after))
        variants = [copy.deepcopy(after) for _ in range(3)]
        variants[0]["cases"][0].pop("plugin_data")
        variants[1]["cases"][:2] = reversed(variants[1]["cases"][:2])
        variants[2]["cases"][-1]["id"] = variants[2]["cases"][0]["id"]
        for variant in variants:
            self.assertFalse(oracles.preserved_json(before, variant))


class NativeEvidence(unittest.TestCase):
    def setUp(self):
        temp = tempfile.TemporaryDirectory()
        self.addCleanup(temp.cleanup)
        self.root = Path(temp.name)
        (self.root / "greet.py").write_text("broken")
        self.before = common.inventory(self.root)
        (self.root / "greet.py").write_text("fixed")
        self.case = SUITE["cases"][0]
        command = self.case["checks"][0]["command"]
        self.report = {"completed": True, "work": {"requests": [{"role": "coder", "provider_usage": None}]},
                       "execution": {"pending_exact_checks": [], "mutation_step": 2, "actions": [
                           {"tool": "exec", "arguments": {"command": command}, "executed": True, "succeeded": False, "result_step": 1},
                           {"tool": "patch_file", "arguments": {"path": "greet.py"}, "executed": True, "succeeded": True, "result_step": 2},
                           {"tool": "exec", "arguments": {"command": command}, "executed": True, "succeeded": True, "result_step": 3},
                           {"tool": "done", "executed": True, "result": "OK: done", "arguments": {
                               "summary": f"greet.py. Before: FAILED — `{command}`. After: PASSED — `{command}`.",
                               "remaining_acceptance": []}, "result_step": 4}]}}
        self.process = {"exit_code": 0, "timed_out": False}

    def assess(self):
        return evidence.assess(self.case, self.before, self.root, self.report, self.process, {"passed": True})

    def test_full_filesystem_scope_retains_untracked_runtime_files(self):
        self.assertTrue(self.assess()["strict_all_files_qualified"])
        (self.root / ".spiral-coder").mkdir()
        (self.root / ".spiral-coder/progress.json").write_text("{}")
        result = self.assess()
        self.assertTrue(result["behavior_and_handoff_qualified"])
        self.assertFalse(result["strict_all_files_qualified"])
        self.assertEqual(result["out_of_scope_files"], [".spiral-coder/progress.json"])
        self.assertIsNone(result["wire_requests"][0]["provider_usage"])

    def test_prose_and_runtime_done_cannot_replace_native_exact_check(self):
        self.report["execution"]["actions"][2]["arguments"]["command"] = "echo passed"
        result = self.assess()
        self.assertTrue(result["runtime_completed"])
        self.assertFalse(result["native"]["fresh_exact_checks"])
        self.assertFalse(result["behavior_and_handoff_qualified"])

    def test_later_shell_mutation_invalidates_prior_file_check(self):
        self.report["execution"]["mutation_step"] = 5
        self.assertFalse(self.assess()["native"]["fresh_exact_checks"])

    def test_failed_process_and_report_omission_remain_incomplete(self):
        self.process["timed_out"] = True
        self.assertFalse(self.assess()["behavior_and_handoff_qualified"])
        self.process["timed_out"] = False
        self.report["execution"]["actions"][-1]["arguments"]["summary"] = "greet.py fixed and tests pass"
        self.assertFalse(self.assess()["report_fields"]["before_failed"])
        self.assertFalse(self.assess()["behavior_and_handoff_qualified"])


class FrozenRunner(unittest.TestCase):
    def test_selection_and_invalid_input_fail_before_provider_work(self):
        common.validate(SUITE, MATRIX)
        self.assertEqual([r["id"] for r in common.select(SUITE["cases"], ["replay-json", "large-file", "large-file"])],
                         ["large-file", "replay-json"])
        with self.assertRaisesRegex(ValueError, "Unknown IDs"):
            common.select(SUITE["cases"], ["missing"])
        broken = copy.deepcopy(SUITE)
        broken["cases"][0]["workspace"] = "../../escape"
        with self.assertRaisesRegex(ValueError, "relative"):
            common.validate(broken, MATRIX)

    def test_timeout_keeps_partial_log_and_stops_the_process(self):
        with tempfile.TemporaryDirectory() as temp:
            code, log, expired = common.run_process(
                [sys.executable, "-S", "-u", "-c", "import time; print('partial'); time.sleep(20)"],
                temp, common.clean_env(), 0.2)
        self.assertTrue(expired)
        self.assertNotEqual(code, 0)
        self.assertIn("partial", log)

    def test_child_environment_only_receives_selected_credential(self):
        with mock.patch.dict(os.environ, {"OPENAI_API_KEY": "other-secret", "GEMINI_API_KEY": "selected-secret", "CARGO_TARGET_DIR": "/shared"}):
            env = common.clean_env()
        self.assertNotIn("OPENAI_API_KEY", env)
        self.assertNotIn("GEMINI_API_KEY", env)
        self.assertNotIn("CARGO_TARGET_DIR", env)

    def test_started_episode_rejected_before_launch_even_after_failure(self):
        from types import SimpleNamespace
        declaration = {"profiles": MATRIX["profiles"][:1], "cases": SUITE["cases"][:1]}
        with tempfile.TemporaryDirectory() as temp:
            out = Path(temp)
            common.write_json(out / "matrix.json", MATRIX)
            folder = out / "episodes/gemini-lite/large-file"
            folder.mkdir(parents=True)
            (folder / "started.json").write_text("{}")
            with mock.patch.object(runner, "load_frozen", return_value=declaration), mock.patch.object(runner, "run_process") as launch:
                with self.assertRaisesRegex(ValueError, "already started"):
                    runner.run(SimpleNamespace(out=out, profile=[], case=[]))
                launch.assert_not_called()


if __name__ == "__main__":
    unittest.main()
