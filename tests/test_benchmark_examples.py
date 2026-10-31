"""Small examples and role-aware freezing: no live provider calls."""
import copy
import json
import os
from pathlib import Path
import shutil
import sys
import tempfile
from types import SimpleNamespace
import unittest
from unittest import mock

ROOT = Path(__file__).resolve().parents[1]
sys.path.insert(0, str(ROOT / "scripts"))
from coupled_bench import common, oracles, roles, runner

FIXTURES = ROOT / "benchmarks/examples-v1"
SUITE = common.read_json(FIXTURES / "suite.json")
MATRIX = common.read_json(FIXTURES / "matrix.json")


class ExampleContracts(unittest.TestCase):
    def test_every_broken_baseline_fails_and_reference_passes_original_tests(self):
        for case in SUITE["cases"]:
            with self.subTest(case=case["id"]), tempfile.TemporaryDirectory() as temp:
                root = Path(temp) / "workspace"
                shutil.copytree(FIXTURES / case["workspace"], root)
                baseline = oracles.score(case, FIXTURES, root, sys.executable)
                self.assertFalse(baseline["passed"])
                self.assertFalse(baseline["checks"]["independent_check_0"])
                self.assertTrue(baseline["checks"]["original_test_count"])
                shutil.copytree(FIXTURES / case["reference"], root, dirs_exist_ok=True)
                before = common.inventory(root)
                self.assertTrue(oracles.score(case, FIXTURES, root, sys.executable)["passed"])
                self.assertEqual(common.inventory(root), before)

    def test_source_only_and_test_rewrites_cannot_qualify(self):
        for case in SUITE["cases"]:
            with self.subTest(case=case["id"]), tempfile.TemporaryDirectory() as temp:
                root = Path(temp) / "workspace"
                shutil.copytree(FIXTURES / case["workspace"], root)
                source = case["editable"][0]
                shutil.copy2(FIXTURES / case["reference"] / source, root / source)
                self.assertFalse(oracles.score(case, FIXTURES, root, sys.executable)["passed"])
                for test in (root / "tests").glob("*.py"):
                    test.write_text("# removed\n")
                result = oracles.score(case, FIXTURES, root, sys.executable)
                self.assertFalse(result["checks"]["supplied_files_preserved"])
                self.assertTrue(result["checks"]["original_test_count"])
                self.assertFalse(result["passed"])

    def test_equivalent_source_need_not_match_reference_bytes(self):
        case = SUITE["cases"][0]
        with tempfile.TemporaryDirectory() as temp:
            root = Path(temp) / "workspace"
            shutil.copytree(FIXTURES / case["workspace"], root)
            shutil.copytree(FIXTURES / case["reference"], root, dirs_exist_ok=True)
            with (root / "settings.py").open("a") as handle:
                handle.write("\n# A different but equivalent implementation artifact.\n")
            result = oracles.score(case, FIXTURES, root, sys.executable)
            self.assertTrue(result["passed"])
            self.assertFalse(result["source_matches_reference"])


class RoleContracts(unittest.TestCase):
    def test_new_and_old_matrices_validate_without_changing_old_role_defaults(self):
        common.validate(SUITE, MATRIX)
        common.validate(SUITE, common.read_json(FIXTURES / "matrix-mistral.json"))
        common.validate(SUITE, common.read_json(FIXTURES / "matrix-openai.json"))
        old_suite = common.read_json(ROOT / "benchmarks/coding-v1/suite.json")
        old_matrix = common.read_json(ROOT / "benchmarks/coding-v1/matrix.json")
        common.validate(old_suite, old_matrix)
        with mock.patch.dict(os.environ, {"GEMINI_API_KEY": "gemini-test-key"}, clear=True):
            command, env, _ = roles.command_and_env(Path("binary"), Path("folder"),
                                                   old_matrix["profiles"][0], old_suite["cases"][0], old_matrix)
        self.assertNotIn("--observer-provider", command)
        self.assertNotIn("--transport-repair-requests", command)
        self.assertEqual(env["GEMINI_API_KEY"], "gemini-test-key")

    def test_invalid_roles_and_budgets_fail_closed(self):
        for change in (lambda s, m: m["profiles"][0].pop("observer"),
                       lambda s, m: m["profiles"][0]["observer"].update(provider="missing"),
                       lambda s, m: m.update(same_model_both_roles=True),
                       lambda s, m: m.update(observer_max_tokens=True),
                       lambda s, m: s["cases"][0].update(observer_reserved_requests=24),
                       lambda s, m: s["cases"][0].update(transport_repair_requests=9),
                       lambda s, m: s["cases"][0].update(coder_chunk_requests=3)):
            suite, matrix = copy.deepcopy(SUITE), copy.deepcopy(MATRIX)
            change(suite, matrix)
            with self.assertRaises(ValueError):
                common.validate(suite, matrix)

    def test_only_both_selected_credentials_reach_the_process(self):
        with mock.patch.dict(os.environ, {"OPENAI_API_KEY": "coder-test-key", "MISTRAL_API_KEY": "observer-test-key",
                                          "GEMINI_API_KEY": "unrelated-test-key", "SPIRAL_CODER_MODEL": "unrelated"}, clear=True):
            command, env, secrets = roles.command_and_env(Path("binary"), Path("folder"),
                                                        MATRIX["profiles"][0], SUITE["cases"][0], MATRIX)
        self.assertEqual(secrets, ["coder-test-key", "observer-test-key"])
        self.assertNotIn("GEMINI_API_KEY", env)
        self.assertNotIn("SPIRAL_CODER_MODEL", env)
        self.assertEqual(command[command.index("--observer-model") + 1], "mistral-large-4")
        self.assertEqual(command[command.index("--observer-reserved-requests") + 1], "6")
        self.assertEqual(command[command.index("--transport-repair-requests") + 1], "4")
        self.assertFalse(any(secret in " ".join(command) for secret in secrets))

    def test_blank_observer_credential_fails_before_launch(self):
        with mock.patch.dict(os.environ, {"OPENAI_API_KEY": "coder-test-key", "MISTRAL_API_KEY": "  "}, clear=True):
            with self.assertRaisesRegex(ValueError, "MISTRAL_API_KEY"):
                roles.command_and_env(Path("binary"), Path("folder"), MATRIX["profiles"][0], SUITE["cases"][0], MATRIX)

    def report(self):
        return {
            "roles": {"coder": {"provider": "openai", "transport": "openai-compatible", "model": "gpt-5.6-luna",
                                "endpoint_origin": "https://api.openai.com", "temperature": 0, "max_tokens": 32768, "timeout_seconds": 90},
                      "observer": {"provider": "mistral", "transport": "mistral", "model": "mistral-large-4",
                                   "endpoint_origin": "https://api.mistral.ai", "temperature": 0.2, "max_tokens": 8192, "timeout_seconds": 90}},
            "same_model_both_roles": False, "coder_chunk_requests": 8, "role_requests": {"coder": 18, "observer": 7},
            "work": {"request_limit": 24, "policy": {"transport_repair_limit": 4, "observer_reserved_requests": 6},
                     "requests": [{"role": "coder", "charge": "task", "status": "completed"}] * 18
                     + [{"role": "observer", "charge": "task", "status": "completed"}] * 6
                     + [{"role": "observer", "charge": "transport", "status": "completed", "transport_rejection": "observer_envelope"}]}}

    def test_transport_is_additional_but_limited_and_requires_completed_framing_rejection(self):
        report = self.report()
        def check():
            return roles.configuration_evidence(report, MATRIX["profiles"][0], SUITE["cases"][0], MATRIX)
        self.assertTrue(check()["valid"])
        report["work"]["requests"][-1]["status"] = "output_limited"
        self.assertFalse(check()["valid"])
        self.assertFalse(check()["checks"]["transport_eligibility"])
        report["work"]["requests"][-1]["status"] = "completed"
        report["roles"]["observer"]["model"] = "wrong"
        self.assertFalse(check()["valid"])

    def test_missing_requests_and_overusing_coder_reservation_cannot_qualify(self):
        for report in ({}, self.report()):
            if report:
                report["work"]["requests"].append({"role": "coder", "charge": "task", "status": "completed"})
                report["role_requests"]["coder"] += 1
            self.assertFalse(roles.configuration_evidence(report, MATRIX["profiles"][0], SUITE["cases"][0], MATRIX)["valid"])


class FrozenExamples(unittest.TestCase):
    def test_freeze_rejects_mutated_inputs_and_redacts_both_credentials(self):
        with tempfile.TemporaryDirectory() as temp:
            out = Path(temp) / "frozen"
            result = runner.prepare(SimpleNamespace(suite=FIXTURES / "suite.json", matrix=FIXTURES / "matrix.json",
                                                    binary=Path(sys.executable), out=out, runtime_commit="test-only",
                                                    case=["env-bool"], profile=["luna-large"]))
            self.assertEqual(result["maximum_wire_requests"], 28)
            self.assertEqual(result["provider_calls"], 0)
            folder = out / "episodes/luna-large/env-bool"
            def fake_run(command, cwd, env, timeout):
                self.assertEqual(env["MISTRAL_API_KEY"], "observer-test-key")
                (folder / "run").mkdir()
                (folder / "run/report.json").write_text('"coder-test-key observer-test-key"')
                return 0, "coder-test-key observer-test-key", False
            with mock.patch.object(runner, "load_frozen", return_value=common.read_json(out / "predeclared.json")), \
                 mock.patch.object(runner, "run_process", side_effect=fake_run), \
                 mock.patch.dict(os.environ, {"OPENAI_API_KEY": "coder-test-key", "MISTRAL_API_KEY": "observer-test-key"}, clear=True):
                runner.run(SimpleNamespace(out=out, profile=[], case=[]))
                with self.assertRaisesRegex(ValueError, "already started"):
                    runner.run(SimpleNamespace(out=out, profile=[], case=[]))
            self.assertEqual((folder / "console.log").read_text(), "[REDACTED] [REDACTED]")
            self.assertEqual(json.loads((folder / "run/report.json").read_text()), "[REDACTED] [REDACTED]")
            (out / "inputs/cases/env-bool/task.txt").write_text("changed")
            with self.assertRaisesRegex(ValueError, "inputs changed"):
                runner.load_frozen(out, require_driver=False)


if __name__ == "__main__":
    unittest.main()
