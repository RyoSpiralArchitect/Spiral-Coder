"""Protected reproducer for the coupled-status public contract."""
import json
import os
from pathlib import Path
import re
import subprocess
import tempfile
import unittest
from scripts.status_fixture import make, expected


class StatusVisible(unittest.TestCase):
    def test_status_summarizes_completed_and_interrupted_records(self):
        subprocess.run(['cargo', 'build', '--locked', '--quiet', '--bin', 'spiral-coder'], check=True)
        binary = Path(os.environ['CARGO_TARGET_DIR']) / 'debug/spiral-coder'
        help_text = subprocess.check_output([binary, '--help'], text=True)
        self.assertRegex(help_text, r'(?m)^\s+coupled-status(?:\s|$)', 'coupled-status command missing from --help')
        routed = subprocess.check_output(['node', '-e', "const l=require('./npm/spiral-coder/lib/launcher.cjs');process.stdout.write(JSON.stringify(l.argumentsFor('spiral',['coupled-status','--run-dir','x'],process.cwd())))"], text=True)
        self.assertEqual(json.loads(routed), ['coupled-status','--run-dir','x'], 'installed command routing')
        with tempfile.TemporaryDirectory() as tmp:
            for state in ('completed', 'running', 'interrupted', 'unknown', 'stopped'):
                root = Path(tmp) / state
                make(root, state)
                result = subprocess.run([binary, 'coupled-status', '--run-dir', root], capture_output=True, text=True)
                self.assertEqual(result.returncode, 0, result.stderr)
                self.assertEqual(json.loads(result.stdout), expected(state), state)
            bad = Path(tmp) / 'bad'
            m = make(bad)
            m['task_requests'] += 1
            (bad / 'iterations.json').write_text(json.dumps(m))
            result = subprocess.run([binary, 'coupled-status', '--run-dir', bad], capture_output=True, text=True)
            self.assertNotEqual(result.returncode, 0)
            self.assertEqual(result.stdout, '')
            self.assertTrue(result.stderr.strip())


if __name__ == '__main__':
    unittest.main(verbosity=2)
