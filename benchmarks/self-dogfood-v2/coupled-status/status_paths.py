"""Public path-boundary controls; absence is not a malformed or symlinked record."""
import copy
import json
import os
from pathlib import Path
import shutil
import subprocess
import tempfile
import unittest
from scripts.status_fixture import make, expected, put


class StatusPaths(unittest.TestCase):
    @classmethod
    def setUpClass(cls):
        subprocess.run(['cargo', 'build', '--locked', '--quiet', '--bin', 'spiral-coder'], check=True)
        cls.binary = Path(os.environ['CARGO_TARGET_DIR']) / 'debug/spiral-coder'

    def setUp(self):
        help_text = subprocess.run([self.binary, "--help"], capture_output=True, text=True, check=True).stdout
        self.assertIn("coupled-status", help_text, "coupled-status command missing from --help")

    def invoke(self, root, valid, label):
        result = subprocess.run([self.binary, 'coupled-status', '--run-dir', root], capture_output=True, text=True)
        self.assertEqual(result.returncode == 0, valid, f'{label}: {result.stderr}')
        if valid:
            return json.loads(result.stdout)
        self.assertEqual(result.stdout, '', label)
        self.assertTrue(result.stderr.strip(), label)

    def test_missing_pending_components_are_unknown(self):
        with tempfile.TemporaryDirectory() as tmp:
            for state in ('running', 'interrupted'):
                for missing in ('iteration-02', 'iteration-02/state', 'iteration-02/state/tui-work.json'):
                    label = f'{state}/{missing}'
                    with self.subTest(case=label):
                        root = Path(tmp) / str(len(list(Path(tmp).iterdir())))
                        manifest = make(root, state)
                        target = root / missing
                        if target.is_dir(): shutil.rmtree(target)
                        else: target.unlink()
                        if state == 'interrupted':
                            manifest['task_requests'] = manifest['wire_requests'] = None
                            manifest['interrupted_iteration']['task_requests'] = None
                            manifest['interrupted_iteration']['wire_requests'] = None
                            put(root, 'iterations.json', manifest)
                        want = copy.deepcopy(expected(state))
                        for key in ('task_id', 'task_requests', 'wire_requests', 'in_flight_requests'):
                            want['pending'][key] = None
                        for key in ('task_used', 'wire_used', 'task_remaining', 'wire_remaining'):
                            want['requests'][key] = None
                        self.assertEqual(self.invoke(root, True, label), want, label)

    def test_malformed_existing_pending_is_rejected(self):
        with tempfile.TemporaryDirectory() as tmp:
            for state in ('running', 'interrupted'):
                root = Path(tmp) / state; make(root, state)
                (root / 'iteration-02/state/tui-work.json').write_text('{')
                with self.subTest(state=state): self.invoke(root, False, state)

    def test_symlink_components_even_dangling_are_rejected(self):
        with tempfile.TemporaryDirectory() as tmp:
            count = 0
            for state in ('completed', 'running', 'interrupted'):
                directory = 'iteration-01' if state == 'completed' else 'iteration-02'
                for component in (directory, directory + '/state', directory + '/state/tui-work.json'):
                    for dangling in (False, True):
                        label = f'{state}/{component}/dangling={dangling}'
                        with self.subTest(case=label):
                            root = Path(tmp) / str(count); count += 1; make(root, state)
                            target = root / component; outside = Path(tmp) / f'outside-{count}'
                            is_dir = target.is_dir(); target.rename(outside)
                            if dangling:
                                if is_dir: shutil.rmtree(outside)
                                else: outside.unlink()
                            target.symlink_to(outside, target_is_directory=is_dir)
                            self.invoke(root, False, label)


if __name__ == '__main__': unittest.main(verbosity=2)
