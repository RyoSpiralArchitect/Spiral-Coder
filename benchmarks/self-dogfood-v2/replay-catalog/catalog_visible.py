"""Protected visible check. Exercise the locally built CLI against the stocked replay spec."""
import json
import os
from pathlib import Path
import re
import subprocess
import unittest


class CatalogVisible(unittest.TestCase):
    def test_catalog_lists_one_filtered_case(self):
        subprocess.run(["cargo", "build", "--locked", "--quiet", "--bin", "spiral-coder"], check=True)
        binary = Path(os.environ["CARGO_TARGET_DIR"]) / "debug/spiral-coder"
        help_text = subprocess.check_output([binary, "--help"], text=True)
        self.assertTrue(re.search(r"(?m)^\s+tui-replay-list(?:\s|$)", help_text),
                        "catalog command missing from --help")
        result = subprocess.run([binary, "tui-replay-list", "--filter", "PREFS", "--max-cases", "1"],
                                capture_output=True, text=True, check=True)
        self.assertEqual(json.loads(result.stdout), {
            "version": 1, "total": 1, "cases": [{"id": "pane-prefs-observer-suggestion",
            "tags": ["tui", "observer", "suggestion", "prefs"]}]})


if __name__ == "__main__":
    unittest.main(verbosity=2)
