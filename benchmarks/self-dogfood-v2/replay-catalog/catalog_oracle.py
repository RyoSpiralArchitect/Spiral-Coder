"""Held-out tests of the candidate-built executable, including empty directories."""
import hashlib
import json
import os
from pathlib import Path
import re
import stat
import subprocess
import sys
import tempfile
import unittest

BINARY = Path(sys.argv.pop()).resolve()


def tree(root):
    rows = {}
    for p in sorted(root.rglob("*")):
        mode = p.lstat().st_mode
        body = (os.readlink(p) if stat.S_ISLNK(mode) else
                hashlib.sha256(p.read_bytes()).hexdigest() if stat.S_ISREG(mode) else None)
        rows[p.relative_to(root)] = mode, body
    return rows


class Catalog(unittest.TestCase):
    def setUp(self):
        temp = tempfile.TemporaryDirectory()
        self.addCleanup(temp.cleanup)
        self.parent = Path(temp.name)
        self.cwd = self.parent / "caller"
        self.cwd.mkdir()
        self.root = self.parent / "project with spaces"
        self.root.mkdir()
        self.spec = self.root / ".spiral-coder/tui_replay.json"
        self.spec.parent.mkdir()
        self.env = dict(os.environ, SPIRAL_CODER_REPLAY_OUTPUT_ROOT=str(self.parent / "unused-output"))
        self.rows = [
            {"id": "Zulu", "tags": ["FoCUS", "z"]},
            {"id": "empty", "tags": []},
            {"id": "FOCUS-second", "tags": ["a", "B"]},
        ]
        # Valid at input parsing, but unable to execute a replay. Listing must not execute it.
        self.payload = {"version": 1, "cases": [
            dict(r, prompt="PRIVATE_PROMPT", observer_response="PRIVATE_RESPONSE",
                 checks=[{"kind": "suggestion_parsed"}]) for r in self.rows]}
        self.spec.write_text(json.dumps(self.payload))
        help_text = subprocess.check_output([BINARY, "--help"], env=self.env, cwd=self.cwd, text=True)
        self.assertTrue(re.search(r"(?m)^\s+tui-replay-list(?:\s|$)", help_text),
                        "catalog command missing from --help")

    def listing(self, *args):
        before = tree(self.parent)
        result = subprocess.run([BINARY, "tui-replay-list", *args], cwd=self.cwd, env=self.env,
                                capture_output=True, text=True, timeout=15)
        self.assertEqual(tree(self.parent), before, "listing changed a file, mode, or directory")
        return result

    def valid(self, expected, *args):
        result = self.listing(*args)
        self.assertEqual(result.returncode, 0, result.stderr)
        value = json.loads(result.stdout)
        self.assertEqual(value, {"version": 1, "total": len(expected), "cases": expected})
        self.assertIs(type(value["version"]), int)
        self.assertIs(type(value["total"]), int)
        self.assertNotIn("PRIVATE_", result.stdout + result.stderr)

    def invalid(self, marker, *args):
        result = self.listing(*args)
        self.assertNotEqual(result.returncode, 0)
        self.assertFalse(result.stdout.strip())
        self.assertIn(marker, result.stderr)

    def test_order_and_default_spec_under_tool_root(self):
        self.valid(self.rows, "--tool-root", str(self.root))

    def test_filter_matches_tag_and_id_in_spec_order(self):
        self.valid([self.rows[0], self.rows[2]], "-C", str(self.root), "--filter", "fOcUs")

    def test_cap_applies_after_filter(self):
        self.valid(self.rows[2:], "--spec", str(self.spec), "--filter", "SeCoNd", "--max-cases", "1")

    def test_zero_means_first(self):
        self.valid(self.rows[:1], "--spec", str(self.spec), "--max-cases", "0")

    def test_relative_custom_spec_and_absent_tags(self):
        del self.payload["cases"][1]["tags"]
        (self.root / "other.json").write_text(json.dumps(self.payload))
        self.valid([self.rows[1]], "-C", str(self.root), "--spec", "other.json", "--filter", "EMPTY")

    def test_no_matches(self):
        self.invalid("0 cases", "--spec", str(self.spec), "--filter", "no-such-tag")

    def test_malformed(self):
        self.spec.write_text("{")
        self.invalid("parse", "--spec", str(self.spec))

    def test_version(self):
        self.payload["version"] = 999
        self.spec.write_text(json.dumps(self.payload))
        self.invalid("version", "--spec", str(self.spec))

    def test_empty_cases(self):
        self.spec.write_text('{"cases":[]}')
        self.invalid("no cases", "--spec", str(self.spec))

    def test_invalid_unselected_case_is_still_validated(self):
        self.payload["cases"][1]["checks"] = []
        self.spec.write_text(json.dumps(self.payload))
        self.invalid("checks", "--spec", str(self.spec), "--filter", "Zulu")

    def test_usage_is_documented(self):
        doc = Path("docs/evaluation.md").read_text()
        self.assertRegex(doc, r"(?:spiral|spiral-coder) tui-replay-list\b")
        for word in ("--filter", "--max-cases", "JSON"):
            self.assertIn(word, doc)


if __name__ == "__main__":
    unittest.main(verbosity=2)
