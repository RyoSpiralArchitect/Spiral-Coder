import importlib.util
import contextlib
import io
import json
import os
from pathlib import Path
import sys
import tempfile
import unittest
from unittest import mock

spec = importlib.util.spec_from_file_location("pilot", Path(__file__).parents[1] / "scripts/run_coupled_pilot.py")
pilot = importlib.util.module_from_spec(spec)
spec.loader.exec_module(pilot)


class PilotOracles(unittest.TestCase):
    def run_command(self, code, timeout=2):
        with tempfile.TemporaryDirectory() as root:
            return pilot.run_bounded([sys.executable, "-S", "-u", "-c", code], cwd=root, env=os.environ.copy(), timeout=timeout)

    def test_timeout_retains_output_and_failure(self):
        code, output, expired = self.run_command("import time; print('before timeout'); time.sleep(30)", timeout=0.3)
        self.assertTrue(expired)
        self.assertNotEqual(code, 0)
        self.assertIn("before timeout", output)

    def test_failed_oracle_retains_output(self):
        code, output, expired = self.run_command("print('failed assertion'); raise SystemExit(2)")
        self.assertEqual(code, 2)
        self.assertFalse(expired)
        self.assertIn("failed assertion", output)

    def test_preservation_keeps_nested_extensions_and_rejects_duplicate_ids(self):
        baseline = {"version": 1, "cases": [{"id": "old", "based_on": ["proof"], "extension": {"retain": True}}]}
        self.assertTrue(pilot.preserved(baseline, {**baseline, "cases": baseline["cases"] + [{"id": "new"}]}))
        self.assertFalse(pilot.preserved(baseline, {"version": 1, "cases": [{"id": "old", "based_on": []}]}))
        self.assertFalse(pilot.preserved(baseline, {**baseline, "cases": baseline["cases"] * 2}))


class PilotCaseSelection(unittest.TestCase):
    def setUp(self):
        directory = tempfile.TemporaryDirectory()
        self.addCleanup(directory.cleanup)
        self.root = Path(directory.name)
        self.repo = self.root / "repo"
        (self.repo / "tests/fixtures").mkdir(parents=True)
        self.cases = []
        for name in ["alpha", "beta", "gamma"]:
            (self.repo / name).mkdir()
            (self.repo / name / "note.txt").write_text(name)
            self.cases.append({"id": name, "tool_root": name, "prompt": "Inspect note.txt.", "max_requests": 2})
        self.spec = json.dumps({"version": 1, "cases": self.cases}).encode()
        (self.repo / "tests/fixtures/coupled-loop-v1.json").write_bytes(self.spec)
        self.binary = self.root / "fake-binary"
        self.binary.write_bytes(b"Never executed: run_bounded is mocked")
        self.out = self.root / "receipts"
        self.invoked = []

    def fake_run(self, command, *, cwd, env, timeout):
        case = json.loads(Path(command[command.index("--case-spec") + 1]).read_text())
        self.invoked.append(case)
        self.assertEqual(command[command.index("--max-requests") + 1], "2")
        out = Path(command[command.index("--out-dir") + 1])
        out.mkdir()
        (out / "report.json").write_text(json.dumps({"completed": True, "work": {"requests": []}, "execution": {"pending_exact_checks": []}}))
        (out / "checks.json").write_text(json.dumps({"ok": True}))
        return 0, "offline", False

    def invoke(self, flags=(), *, credential=True):
        argv = ["pilot", "--binary", str(self.binary), "--repo", str(self.repo), "--out", str(self.out), "--provider", "openai", "--model", "offline", *flags]
        with mock.patch.object(sys, "argv", argv), mock.patch.dict(os.environ, {"OPENAI_API_KEY": "offline"} if credential else {}, clear=True), mock.patch.object(pilot, "run_bounded", side_effect=self.fake_run), contextlib.redirect_stdout(io.StringIO()):
            pilot.main()

    def assert_selection(self, expected):
        self.assertEqual([case["id"] for case in self.invoked], expected)
        self.assertEqual(self.invoked, [case for case in self.cases if case["id"] in expected])
        self.assertEqual((self.out / "spec.json").read_bytes(), self.spec)
        for file in ["metadata.json", "summary.json"]:
            data = json.loads((self.out / file).read_text())
            self.assertEqual(data["selected_case_ids"], expected)
        summary = json.loads((self.out / "summary.json").read_text())
        self.assertEqual([case["id"] for case in summary["cases"]], expected)
        self.assertTrue(all(case["ok"] for case in summary["cases"]))

    def test_default_runs_every_case_and_records_selection(self):
        self.invoke()
        self.assert_selection(["alpha", "beta", "gamma"])

    def test_repeated_reverse_selection_runs_once_in_suite_order(self):
        self.invoke(["--case", "gamma", "--case", "alpha", "--case", "gamma"])
        self.assert_selection(["alpha", "gamma"])
        self.assertFalse((self.out / "beta").exists())

    def test_unknown_case_fails_before_output_or_execution_without_a_key(self):
        errors = io.StringIO()
        with contextlib.redirect_stderr(errors), self.assertRaises(SystemExit) as caught:
            self.invoke(["--case", "alpha", "--case", "missing-case"], credential=False)
        self.assertNotEqual(caught.exception.code, 0)
        self.assertIn("Unknown case ID", errors.getvalue())
        self.assertIn("missing-case", errors.getvalue())
        self.assertFalse(self.out.exists())
        self.assertEqual(self.invoked, [])

    def test_selection_cannot_overwrite_existing_receipts(self):
        self.out.mkdir()
        receipt = self.out / "metadata.json"
        receipt.write_text("old receipt")
        with self.assertRaisesRegex(SystemExit, "preserve receipts"):
            self.invoke(["--case", "alpha"])
        self.assertEqual(receipt.read_text(), "old receipt")
        self.assertEqual(self.invoked, [])


if __name__ == "__main__":
    unittest.main()
