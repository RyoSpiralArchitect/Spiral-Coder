"""Visible public regression for the first interrupted iteration."""
import json
import os
from pathlib import Path
import subprocess
import tempfile
import unittest

from scripts.status_fixture import make, put


class FirstInterruption(unittest.TestCase):
    def test_a_durable_first_ledger_counts_before_any_iteration_finalizes(self):
        subprocess.run(["cargo", "build", "--locked", "--quiet", "--bin", "spiral-coder"], check=True)
        binary = Path(os.environ["CARGO_TARGET_DIR"]) / "debug/spiral-coder"
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            manifest = make(root)
            first = manifest["iterations"].pop()
            manifest.update(completed=False, stop_reason="interrupted", interrupted_iteration={
                key:first[key] for key in ("directory", "task_requests", "wire_requests")
            })
            put(root, "iterations.json", manifest)
            result = subprocess.run([binary, "coupled-status", "--run-dir", root], capture_output=True, text=True)
            self.assertEqual(result.returncode, 0, f"durable first interruption must be readable: {result.stderr}")
            status = json.loads(result.stdout)
            self.assertEqual(status["iterations"], [])
            self.assertEqual(status["status"], "stopped")
            self.assertEqual(status["pending"]["task_id"], first["task_id"])
            self.assertEqual((status["requests"]["task_used"], status["requests"]["wire_used"]), (2, 3))
