import importlib.util
import os
from pathlib import Path
import sys
import tempfile
import unittest

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


if __name__ == "__main__":
    unittest.main()
