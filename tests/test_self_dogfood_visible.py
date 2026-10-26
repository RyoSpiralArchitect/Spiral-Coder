"""Visible operator fixtures stay frozen in both the candidate and verifier."""
import copy
import unittest

import test_self_dogfood as lane_fixture
from self_dogfood import contracts
from coupled_bench.common import read_json, write_json


class VisibleFixtureTests(unittest.TestCase):
    def setUp(self):
        self.lane = lane_fixture.LaneTests()
        self.lane.setUp()
        self.addCleanup(self.lane.doCleanups)
        lane = self.lane
        (lane.inputs / "visible.rs").write_text("visible behavioral assertion\n")
        (lane.inputs / "register.rs").write_text("register visible test\n")
        lane.spec["visible_fixtures"] = [
            {"source": "visible.rs", "target": "tests/visible_recovery.rs"},
            {"source": "register.rs", "target": "tests/existing.rs", "mode": "append"},
        ]
        write_json(lane.inputs / "task.json", lane.spec)

    def test_visible_files_are_present_in_candidate_and_all_verifiers(self):
        lane = self.lane
        original = lane.fake_check
        observed = []

        def inspect(argv, workspace, *args):
            self.assertEqual((workspace / "tests/visible_recovery.rs").read_text(), "visible behavioral assertion\n")
            self.assertEqual((workspace / "tests/existing.rs").read_text(), "original regression\nregister visible test\n")
            observed.append(workspace)
            return original(argv, workspace, *args)

        lane.fake_check = inspect
        lane.prepare()
        candidate = lane.out / "episode/workspace"
        self.assertEqual((candidate / "tests/visible_recovery.rs").read_text(), "visible behavioral assertion\n")
        self.assertEqual(lane.git("show", lane.commit + ":tests/existing.rs"), "original regression\n")
        declaration, _ = contracts.load_frozen(lane.out, lane.pin)
        targets = ["tests/existing.rs", "tests/visible_recovery.rs"]
        self.assertEqual(declaration["visible_fixture_paths"], targets)
        self.assertTrue(set(targets).issubset(declaration["protected_paths"]))
        ran = lane.execute()
        result = lane.assess(ran["run_receipt_sha256"])
        self.assertTrue(result["qualified_candidate"], result["gates"])
        self.assertEqual(len(observed), 9)  # Three checks on baseline, reference and candidate.
        self.assertEqual(read_json(lane.out / "assessment-001.json")["visible_fixture_paths"], targets)

    def test_candidate_cannot_change_visible_assertion_or_registration(self):
        lane = self.lane
        original = lane.fake_runtime
        for index, target in enumerate(("tests/visible_recovery.rs", "tests/existing.rs")):
            with self.subTest(target=target):
                lane.out = lane.root / f"tamper-{index}"
                lane.prepare()

                def tamper(argv, workspace, *args, **kwargs):
                    result = original(argv, workspace, *args, **kwargs)
                    (workspace / target).write_text("disabled assertion\n")
                    return result

                lane.fake_runtime = tamper
                ran = lane.execute()
                result = lane.assess(ran["run_receipt_sha256"])
                self.assertFalse(result["qualified_candidate"])
                self.assertFalse(result["gates"]["strict_scope"])
                self.assertFalse(result["gates"]["original_protected_files_preserved"])
                self.assertTrue(result["gates"]["independent_checks_passed"])

    def test_frozen_visible_input_is_bound_to_the_external_pin(self):
        lane = self.lane
        lane.prepare()
        (lane.out / "inputs/visible.rs").write_text("weaker assertion\n")
        with self.assertRaisesRegex(ValueError, "Frozen inputs changed"):
            contracts.load_frozen(lane.out, lane.pin)

    def test_fixture_contract_rejects_bad_paths_shapes_and_overlaps(self):
        valid = self.lane.spec["visible_fixtures"][0]
        invalid = [None, {}, [], "visible.rs", [None], [valid] * 9, [valid, valid],
                   [{**valid, "target": "src/recovery.rs"}],
                   [{**valid, "target": "tests/./visible_recovery.rs"}],
                   [{**valid, "target": ".git/config"}],
                   [{**valid, "target": "../escaped.rs"}],
                   [{**valid, "source": None}], [{**valid, "mode": "unknown"}],
                   [{**valid, "editable": True}]]
        for fixtures in invalid:
            with self.subTest(fixtures=fixtures):
                task = copy.deepcopy(self.lane.spec)
                task["visible_fixtures"] = fixtures
                with self.assertRaises(ValueError):
                    contracts.validate(task)
        task = copy.deepcopy(self.lane.spec)
        task["probes"][0]["target"] = valid["target"]
        with self.assertRaisesRegex(ValueError, "disjoint"):
            contracts.validate(task)


if __name__ == "__main__":
    unittest.main()
