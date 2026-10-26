"""Budget receipts remain strict; development milestones never qualify a run."""
import copy
from pathlib import Path
import sys
import unittest

sys.path.insert(0, str(Path(__file__).resolve().parents[1] / "scripts"))
from self_dogfood.measurement import budget_accounting, development_progress


class MeasurementTests(unittest.TestCase):
    def test_transport_has_its_own_cap_and_cannot_hide_semantic_or_network_failures(self):
        spec = {"max_requests": 4, "transport_repair_requests": 1, "observer_reserved_requests": 1}
        rows = [{"id": i, "role": role, "charge": "task"} for i, role in enumerate(["coder"] * 3 + ["observer"])]
        rows.append({"id": 4, "role": "observer", "charge": "transport", "status": "completed", "transport_rejection": "observer_envelope"})
        work = {"request_limit": 4, "policy": {"transport_repair_limit": 1, "observer_reserved_requests": 1}, "requests": rows}
        self.assertTrue(budget_accounting(work, spec)["valid"])
        for change in [lambda w: w["requests"][-1].update(status="stream_interrupted"),
                       lambda w: w["requests"][-1].update(transport_rejection="proof_failure"),
                       lambda w: w["requests"][3].update(role="coder"),
                       lambda w: w["requests"].append({**rows[-1], "id": 5}),
                       lambda w: w["policy"].update(transport_repair_limit=2)]:
            bad = copy.deepcopy(work)
            change(bad)
            self.assertFalse(budget_accounting(bad, spec)["valid"])

    def test_partial_progress_does_not_mask_regression_or_create_qualification(self):
        spec = {"reproducer_command": "visible --exact", "visible_check_id": "visible", "checks":[{"id":"visible", "expected_test_count":1, "baseline_failure_contains":["target_test ... FAILED"]}]}
        report = {"termination": "blocked", "execution": {"actions": [
            {"tool": "exec", "executed": True, "succeeded": False, "arguments": {"command": "visible --exact"}, "result_step": 1, "result":"target_test ... FAILED\ntest result: FAILED. 0 passed; 1 failed; 0 ignored;"},
            {"tool": "patch_file", "executed": True, "succeeded": True, "result_step": 2}]}}
        baseline = [{"id": "visible", "test_summary": {"passed": 0, "failed": 1}}]
        checks = [{"id": "visible", "kind": "improvement", "passed": True, "test_summary": {"passed": 1, "failed": 0}},
                  {"id": "controls", "kind": "regression", "passed": False}]
        gates = dict.fromkeys(["fresh_native_checks", "required_paths_changed", "native_receipts_valid", "pinned_run_receipt", "accepted_done", "no_remaining_acceptance"], False)
        original = copy.deepcopy(gates)
        progress = development_progress(spec, report, checks, baseline, gates)
        self.assertEqual(progress["highest_contiguous_stage"], 3)
        self.assertEqual(progress["first_unmet_stage"], "control_non_regression")
        self.assertEqual(gates, original)
        self.assertNotIn("qualified_candidate", progress)
        report["execution"]["actions"][0]["result"] = "FAILED (exit_code: 101) could not download dependency"
        self.assertEqual(development_progress(spec, report, checks, baseline, gates)["milestones"]["reproduced"], "not_observed")
        report["execution"]["actions"][0]["result_step"] = 3
        self.assertEqual(development_progress(spec, report, checks, baseline, gates)["highest_contiguous_stage"], 0)

    def test_missing_measurement_is_unknown_and_an_unchanged_pass_is_not_behavior_change(self):
        gates = dict.fromkeys(["fresh_native_checks", "required_paths_changed", "native_receipts_valid", "pinned_run_receipt", "accepted_done", "no_remaining_acceptance"], False)
        checks = [{"id": "probe", "kind": "improvement", "passed": False, "test_summary": {"passed": 5, "failed": 1}}]
        progress = development_progress({}, {}, checks, checks, gates)
        self.assertEqual(progress["milestones"]["behavior_changed"], "not_observed")
        self.assertEqual(progress["milestones"]["reproduced"], "not_evaluated")
        self.assertEqual(progress["milestones"]["visible_passed"], "not_evaluated")


if __name__ == "__main__":
    unittest.main()
