"""Same-model real-repo contracts. The cross-family lane keeps its original rules."""
import copy
from pathlib import Path
import shutil
import sys
import tempfile
import unittest

ROOT = Path(__file__).resolve().parents[1]
sys.path.insert(0, str(ROOT / "scripts"))
from coupled_bench.common import inventory, read_json
from self_dogfood.contracts import validate
from self_dogfood.paired import paired_gates, python_summary


class PairedTests(unittest.TestCase):
    def setUp(self):
        self.spec = read_json(ROOT / "benchmarks/self-dogfood-v2/replay-catalog/luna-luna.json")

    def test_role_and_visible_contracts_are_versioned(self):
        validate(self.spec)
        for mutation in (lambda s: s.update(version=1),
                         lambda s: s["roles"]["observer"].update(model="another"),
                         lambda s: s["checks"][1].pop("expected_test_count"),
                         lambda s: s.update(reproducer_command="different check")):
            task = copy.deepcopy(self.spec)
            mutation(task)
            with self.assertRaises(ValueError):
                validate(task)

    def test_counted_unittest_failures_exclude_errors_or_missing_results(self):
        self.assertEqual(python_summary("Ran 11 tests in 0.1s\n\nFAILED (failures=11)\n"),
                         ("FAILED", "0", "11", "0"))
        self.assertEqual(python_summary("Ran 1 test in 0.02s\n\nOK\n"), ("ok", "1", "0", "0"))
        for text in ("Ran 1 test in 0.1s", "Ran 1 test in 0.1s\nOK (skipped=1)",
                     "Ran 1 test in 0.1s\nFAILED (errors=1)", "Ran 1 test in 0.1s\nOK\nbroken"):
            self.assertIsNone(python_summary(text))

    def test_scope_reproduction_report_and_patch_are_independent(self):
        with tempfile.TemporaryDirectory() as tmp:
            root = Path(tmp)
            for name in ("baseline", "scratch", "episode"):
                (root / name).mkdir()
            (root / "baseline/file").write_text("before\n")
            candidate = root / "candidate"
            shutil.copytree(root / "baseline", candidate)
            (candidate / "file").write_text("after\n")
            (root / "episode/candidate.patch").write_text(
                "diff --git a/file b/file\n--- a/file\n+++ b/file\n@@ -1 +1 @@\n-before\n+after\n")
            before = inventory(root / "baseline")
            spec = self.spec
            reproduce, unit = spec["native_checks"]
            report = {"work": {"requests": [{"role": r, "status": "completed"} for r in ("coder", "observer")]},
                      "execution": {"actions": [
                {"tool": "exec", "executed": True, "succeeded": False, "result_step": 1,
                 "arguments": {"command": reproduce}, "result": "catalog command missing from --help"},
                {"tool": "patch_file", "executed": True, "succeeded": True, "result_step": 2},
                {"tool": "done", "executed": True, "result": "OK: done",
                 "arguments": {"summary": f"file changed\n{reproduce}\n{unit}\nBefore: FAILED\nAfter: PASSED"}},
            ]}}

            def gates():
                after = inventory(candidate, include_nested_git=True)
                changed = [p for p in before.keys() | after.keys() if before.get(p) != after.get(p)]
                return paired_gates(root, spec, report, before, after, changed)[0]

            self.assertTrue(all(gates().values()))
            report["work"]["requests"][1]["status"] = "in_flight"
            self.assertFalse(gates()["both_roles_completed_requests"])
            report["execution"]["actions"][0]["result_step"] = 3
            self.assertFalse(gates()["native_reproduction"])
            report["execution"]["actions"][-1]["arguments"]["summary"] += " is NOT claimed"
            self.assertFalse(gates()["authored_report"])
            (candidate / ".tmp/.git").mkdir(parents=True)
            (candidate / ".tmp/.git/config").write_text("ignored")
            self.assertFalse(gates()["patch_replays_all_files"])
