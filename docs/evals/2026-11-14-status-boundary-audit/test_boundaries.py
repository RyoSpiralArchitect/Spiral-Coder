"""Posthoc public-CLI diagnostics; never part of the frozen 24-case oracle.

Run with ``python3 -S test_boundaries.py --binary /absolute/native-binary -v``.
These tests diagnose recorded failure classes, not historical candidate bytes
or fresh-run generalization. Original evaluator files and IDs are untouched.
"""
import argparse
import copy
import hashlib
import importlib.util
import json
import os
from pathlib import Path
import shutil
import stat
import subprocess
import sys
import tempfile
import unittest

sys.dont_write_bytecode = True
BENCHMARK = Path(__file__).resolve().parents[3] / "benchmarks/self-dogfood-v2/coupled-status"
ORIGINAL_TEST_IDS = {
    "first_parent": "StatusOracle.test_wrong_parent",
    "later_parent_positive": "StatusOracle.test_two_rounds",
    "ledger_identity": "StatusOracle.test_wrong_task_id",
    "completed_active": "StatusOracle.test_completion_conflict",
    "pending_absence": "StatusPaths.test_missing_pending_components_are_unknown",
    "pending_dangling": "StatusPaths.test_symlink_components_even_dangling_are_rejected",
    "duplicate_request": "StatusOracle.test_duplicate_request_ids",
}


def load_fixture():
    spec = importlib.util.spec_from_file_location("frozen_status_fixture", BENCHMARK / "status_fixture.py")
    module = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(module)
    return module


def inventory(root):
    """Record links themselves, never follow directory links or lose their target."""
    result = {}

    def visit(path):
        info = path.lstat()
        identity = (info.st_mode, info.st_mtime_ns, info.st_ctime_ns)
        if stat.S_ISLNK(info.st_mode):
            value = ("link", identity, os.readlink(path))
        elif stat.S_ISDIR(info.st_mode):
            value = ("directory", identity)
            with os.scandir(path) as entries:
                for entry in sorted(entries, key=lambda item: item.name):
                    visit(Path(entry.path))
        else:
            value = ("file", identity, hashlib.sha256(path.read_bytes()).hexdigest())
        result[str(path.relative_to(root))] = value

    visit(root)
    return result


class BoundaryDiagnostics(unittest.TestCase):
    binary = None
    maxDiff = None

    @classmethod
    def setUpClass(cls):
        if cls.binary is None:
            raise RuntimeError("Supply --binary; no model or provider is invoked by this runner")
        cls.frozen_bytes = {
            path: path.read_bytes() for path in
            (BENCHMARK / "status_fixture.py", BENCHMARK / "native_work.json")
        }
        cls.fixture = load_fixture()

    @classmethod
    def tearDownClass(cls):
        for path, before in cls.frozen_bytes.items():
            if path.read_bytes() != before:
                raise AssertionError(f"Frozen fixture changed: {path}")

    def setUp(self):
        self.temporary = tempfile.TemporaryDirectory(prefix="status-boundary-")
        self.addCleanup(self.temporary.cleanup)
        self.tree = Path(self.temporary.name)
        self.caller = self.tree / "caller"
        self.caller.mkdir()
        self.env = {
            key: value for key, value in os.environ.items()
            if not any(word in key.upper() for word in
                       ("API_KEY", "TOKEN", "SECRET", "CREDENTIAL", "PASSWORD"))
        }
        self.env.update(HOME=str(self.caller), XDG_CONFIG_HOME=str(self.caller),
                        SPIRAL_CODER_NO_DOTENV="1")

    def make(self, name, state="completed"):
        root = self.tree / name
        manifest = self.fixture.make(root, state)
        self.assertEqual(self.invoke(root), self.fixture.expected(state))
        return root, manifest

    def invoke(self, root, valid=True):
        before = inventory(self.tree)
        try:
            result = subprocess.run(
                [str(self.binary), "coupled-status", "--run-dir", str(root)],
                cwd=self.caller, env=self.env, capture_output=True, text=True, timeout=10,
            )
        finally:
            self.assertEqual(inventory(self.tree), before, "CLI changed the fixture/caller tree")
        if valid:
            self.assertEqual(result.returncode, 0, result.stderr)
            value = json.loads(result.stdout)
            self.assertIsInstance(value, dict)
            return value
        self.assertGreater(result.returncode, 0, "Expected a clean rejection, not success or signal")
        self.assertEqual(result.stdout, "")
        self.assertTrue(result.stderr.strip(), "Rejection must explain itself on stderr")

    def two_rows(self, name):
        root, manifest = self.make(name)
        row = copy.deepcopy(manifest["iterations"][0])
        manifest["iterations"][0].update(completed=False, termination="blocked")
        row.update(iteration=2, directory="iteration-02", task_id="t2", parent_task_id="t1")
        manifest["iterations"].append(row)
        manifest.update(task_requests=4, wire_requests=6)
        self.fixture.put(root, "iteration-02/state/tui-work.json", self.fixture.work("t2"))
        self.fixture.put(root, "iterations.json", manifest)
        result = self.invoke(root)
        self.assertEqual([row["task_id"] for row in result["iterations"]], ["t1", "t2"])
        self.assertEqual(result["requests"]["wire_used"], 6)
        return root, manifest

    def test_first_and_later_parent_linkage(self):
        for later in (False, True):
            with self.subTest(position="later" if later else "first"):
                root, manifest = self.two_rows("later") if later else self.make("first")
                manifest["iterations"][int(later)]["parent_task_id"] = "ghost"
                self.fixture.put(root, "iterations.json", manifest)
                self.invoke(root, valid=False)

    def test_ledger_identity_with_unchanged_counts(self):
        root, _ = self.make("ledger-identity")
        relative = "iteration-01/state/tui-work.json"
        ledger = json.loads((root / relative).read_text())
        original = copy.deepcopy(ledger)
        ledger["task_id"] = "unrelated"
        self.assertEqual({k: v for k, v in ledger.items() if k != "task_id"},
                         {k: v for k, v in original.items() if k != "task_id"})
        self.fixture.put(root, relative, ledger)
        self.invoke(root, valid=False)

    def test_completion_relations(self):
        cases = ("active", "interrupted", "stop_without_completed", "completed_without_stop",
                 "row_termination", "completed_without_last", "nonterminal_completed")
        for case in cases:
            with self.subTest(case=case):
                if case == "nonterminal_completed":
                    root, manifest = self.two_rows(case)
                else:
                    root, manifest = self.make(case, "stopped" if case == "stop_without_completed" else "completed")
                if case == "active":
                    manifest["active_iteration"] = "iteration-02"
                elif case == "interrupted":
                    manifest["interrupted_iteration"] = {
                        "directory": "iteration-02", "task_requests": None, "wire_requests": None,
                    }
                elif case == "stop_without_completed":
                    manifest["stop_reason"] = "completed"
                elif case == "completed_without_stop":
                    manifest["stop_reason"] = "user_refusal"
                elif case == "row_termination":
                    manifest["iterations"][0]["termination"] = "blocked"
                elif case == "completed_without_last":
                    # Keep row completion/termination internally consistent.
                    manifest["iterations"][0].update(completed=False, termination="blocked")
                else:
                    # Keep the row consistent; break only terminal position.
                    manifest["iterations"][0].update(completed=True, termination="completed")
                self.fixture.put(root, "iterations.json", manifest)
                self.invoke(root, valid=False)

    @unittest.skipUnless(hasattr(os, "symlink"), "Platform has no symlink support")
    def test_pending_absence_and_dangling_components(self):
        components = ("iteration-02", "iteration-02/state", "iteration-02/state/tui-work.json")
        for state in ("running", "unknown"):
            for index, relative in enumerate(components):
                with self.subTest(state=state, component=relative):
                    root, _ = self.make(f"{state}-{index}", state)
                    target = root / relative
                    if target.is_dir():
                        shutil.rmtree(target)
                    elif target.exists():
                        target.unlink()
                    target.parent.mkdir(parents=True, exist_ok=True)
                    expected = copy.deepcopy(self.fixture.expected(state))
                    for key in ("task_id", "task_requests", "wire_requests", "in_flight_requests"):
                        expected["pending"][key] = None
                    for key in ("task_used", "wire_used", "task_remaining", "wire_remaining"):
                        expected["requests"][key] = None
                    self.assertEqual(self.invoke(root), expected, "True absence must remain unknown")
                    # Unknown interrupted counts prevent a count mismatch from
                    # masking accidental acceptance of a dangling path component.
                    destination = self.tree / f"missing-target-{state}-{index}"
                    self.assertFalse(destination.exists())
                    target.symlink_to(destination, target_is_directory=index < 2)
                    self.invoke(root, valid=False)

    def test_duplicate_id_preserves_counts_and_charges(self):
        root, _ = self.make("duplicate-request")
        relative = "iteration-01/state/tui-work.json"
        ledger = json.loads((root / relative).read_text())
        original = copy.deepcopy(ledger)
        self.assertGreaterEqual(len(ledger["requests"]), 2)
        ledger["requests"][1]["id"] = ledger["requests"][0]["id"]
        check = copy.deepcopy(ledger)
        check["requests"][1]["id"] = original["requests"][1]["id"]
        self.assertEqual(check, original, "The mutation must change only one existing ID")
        self.fixture.put(root, relative, ledger)
        self.invoke(root, valid=False)


if __name__ == "__main__":
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--binary", type=Path, required=True)
    options, unittest_args = parser.parse_known_args()
    BoundaryDiagnostics.binary = options.binary.resolve(strict=True)
    unittest.main(argv=[sys.argv[0], *unittest_args])
