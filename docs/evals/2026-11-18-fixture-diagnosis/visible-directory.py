"""Check one native interrupted directory and the same input with a gap."""
import json
import os
from pathlib import Path
import subprocess
import tempfile
import unittest

from scripts.status_fixture import make, put


class InterruptedDirectory(unittest.TestCase):
    def test_only_the_next_iteration_directory_is_valid(self):
        subprocess.run(["cargo", "build", "--locked", "--quiet", "--bin", "spiral-coder"], check=True)
        binary = Path(os.environ["CARGO_TARGET_DIR"]) / "debug/spiral-coder"
        with tempfile.TemporaryDirectory() as scratch:
            root = Path(scratch)
            manifest = make(root, "interrupted")
            cli = [binary, "coupled-status", "--run-dir", root]
            valid = subprocess.run(cli, capture_output=True, text=True, timeout=15)
            self.assertEqual(valid.returncode, 0, valid.stderr)
            self.assertEqual(json.loads(valid.stdout)["pending"]["directory"], "iteration-02")
            (root / "iteration-02").rename(root / "iteration-03")
            manifest["interrupted_iteration"]["directory"] = "iteration-03"
            put(root, "iterations.json", manifest)
            gap = subprocess.run(cli, capture_output=True, text=True, timeout=15)
            self.assertNotEqual(gap.returncode, 0)
            self.assertEqual(gap.stdout, "")
            self.assertTrue(gap.stderr.strip())
