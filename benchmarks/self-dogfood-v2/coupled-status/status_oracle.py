"""Independent cases: native totals, partial records, errors and read-only behavior."""
import copy
import json
import os
from pathlib import Path
import subprocess
import sys
import tempfile
import unittest
sys.path.insert(0, str(Path.cwd()))
from scripts.status_fixture import make, expected, put, work

BINARY = Path(sys.argv.pop()).resolve()


def inventory(root):
    return {str(p.relative_to(root)): (('dir',) if p.is_dir() else ('link', os.readlink(p)) if p.is_symlink()
            else ('file', p.stat().st_mode & 0o777, p.read_bytes())) for p in root.rglob('*')}


class StatusOracle(unittest.TestCase):
    def setUp(self):
        help_text = subprocess.check_output([BINARY, '--help'], text=True)
        self.assertRegex(help_text, r'(?m)^\s+coupled-status(?:\s|$)', 'coupled-status command missing from --help')
        self.tmp = tempfile.TemporaryDirectory()
        self.addCleanup(self.tmp.cleanup)
        self.root = Path(self.tmp.name) / 'run'
        self.caller = Path(self.tmp.name) / 'caller'
        self.caller.mkdir()
        self.m = make(self.root)

    def invoke(self, bad=False):
        before = inventory(Path(self.tmp.name))
        env = {k: v for k, v in os.environ.items() if not any(x in k for x in ('API_KEY', 'TOKEN'))}
        env.update(SPIRAL_CODER_NO_DOTENV='1', HOME=str(self.caller))
        p = subprocess.run([BINARY, 'coupled-status', '--run-dir', self.root], cwd=self.caller,
                           env=env, capture_output=True, text=True, timeout=10)
        self.assertEqual(inventory(Path(self.tmp.name)), before)
        if bad:
            self.assertNotEqual(p.returncode, 0)
            self.assertEqual(p.stdout, '')
            self.assertTrue(p.stderr.strip())
            return
        self.assertEqual(p.returncode, 0, p.stderr)
        return json.loads(p.stdout)

    def change(self):
        put(self.root, 'iterations.json', self.m)

    def test_completed(self): self.assertEqual(self.invoke(), expected())
    def test_running(self):
        make(self.root, 'running')
        self.assertEqual(self.invoke(), expected('running'))
    def test_interrupted(self):
        make(self.root, 'interrupted')
        self.assertEqual(self.invoke(), expected('interrupted'))
    def test_unknown(self):
        make(self.root, 'unknown')
        self.assertEqual(self.invoke(), expected('unknown'))
    def test_stop(self):
        make(self.root, 'stopped')
        self.assertEqual(self.invoke(), expected('stopped'))
    def test_bad_json(self):
        (self.root/'iterations.json').write_text('{')
        self.invoke(True)
    def test_version(self):
        self.m['version']=2; self.change(); self.invoke(True)
    def test_missing_final_ledger(self):
        (self.root/'iteration-01/state/tui-work.json').unlink(); self.invoke(True)
    def test_wrong_task_id(self):
        put(self.root,'iteration-01/state/tui-work.json',work('wrong')); self.invoke(True)
    def test_wrong_counts(self):
        self.m['iterations'][0]['task_requests']=3; self.change(); self.invoke(True)
    def test_wrong_total(self):
        self.m['wire_requests']=4; self.change(); self.invoke(True)
    def test_wrong_parent(self):
        self.m['iterations'][0]['parent_task_id']='ghost'; self.change(); self.invoke(True)
    def test_directory(self):
        self.m['iterations'][0]['directory']='../outside'; self.change(); self.invoke(True)
    def test_final_ledger_symlink(self):
        path=self.root/'iteration-01/state/tui-work.json'
        outside=self.caller/'outside.json'; outside.write_bytes(path.read_bytes()); path.unlink(); path.symlink_to(outside)
        self.invoke(True)
    def test_completion_conflict(self):
        self.m['active_iteration']='iteration-02'; self.change(); self.invoke(True)
    def test_budget_overflow(self):
        self.m['max_task_requests']=1; self.change(); self.invoke(True)
    def test_inflight_is_counted(self):
        put(self.root,'iteration-01/state/tui-work.json',work('t1',pending=True))
        self.assertEqual(self.invoke(),expected())
    def test_old_default_charge(self):
        w=work('t1',('task','task','task'))
        for r in w['requests']: r.pop('charge')
        put(self.root,'iteration-01/state/tui-work.json',w)
        self.m['iterations'][0]['task_requests']=3; self.m['task_requests']=3; self.change()
        result=self.invoke(); self.assertEqual(result['requests']['task_used'],3)
        self.assertEqual(result['requests']['task_remaining'],189)
    def test_active_without_ledger(self):
        make(self.root,'running'); (self.root/'iteration-02/state/tui-work.json').unlink()
        result=self.invoke(); self.assertIsNone(result['requests']['task_used']); self.assertIsNone(result['pending']['wire_requests'])
    def test_pending_malformed(self):
        make(self.root,'running'); (self.root/'iteration-02/state/tui-work.json').write_text('{'); self.invoke(True)
    def test_two_rounds(self):
        row=copy.deepcopy(self.m['iterations'][0]); self.m['iterations'][0].update(completed=False,termination='blocked')
        row.update(iteration=2,directory='iteration-02',task_id='t2',parent_task_id='t1')
        self.m['iterations'].append(row); self.m.update(task_requests=4,wire_requests=6); self.change()
        put(self.root,'iteration-02/state/tui-work.json',work('t2'))
        result=self.invoke(); self.assertEqual([r['task_id'] for r in result['iterations']],['t1','t2'])
        self.assertEqual(result['requests']['wire_used'],6)
    def test_duplicate_request_ids(self):
        w=work('t1'); w['requests'][1]['id']=0; put(self.root,'iteration-01/state/tui-work.json',w); self.invoke(True)
    def test_duplicate_task_id(self):
        row=copy.deepcopy(self.m['iterations'][0]); self.m['iterations'][0].update(completed=False,termination='blocked')
        row.update(iteration=2,directory='iteration-02',parent_task_id='t1')
        self.m['iterations'].append(row); self.m.update(task_requests=4,wire_requests=6); self.change()
        put(self.root,'iteration-02/state/tui-work.json',work('t1')); self.invoke(True)
    def test_interrupted_total_conflict(self):
        self.m=make(self.root,'interrupted'); self.m['wire_requests']=4; self.change(); self.invoke(True)


if __name__=='__main__': unittest.main(verbosity=2)
