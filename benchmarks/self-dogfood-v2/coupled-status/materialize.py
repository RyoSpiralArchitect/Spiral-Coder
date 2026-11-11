"""Freeze status inputs and expand the operator reference outside the candidate."""
import argparse,json,shutil,subprocess,sys,tempfile
from pathlib import Path
HERE=Path(__file__).resolve().parent
REPO=HERE.parents[2]
sys.path.insert(0,str(REPO/'scripts'))
from self_dogfood.contracts import git_snapshot

def materialize(out,base):
    if out.exists() or out.is_relative_to(REPO): raise ValueError('Use a new external input directory')
    spec=json.loads((HERE/'luna-luna.json').read_text())
    with tempfile.TemporaryDirectory() as tmp:
        root=Path(tmp)/'source'; git_snapshot(REPO,base,root)
        # S11 declares a test-only baseline correction; no solution is overlaid.
        for name in ['src/config.rs','src/config/env_test_support.rs']:
            target=root/name; target.parent.mkdir(parents=True,exist_ok=True); shutil.copy2(REPO/name,target)
        main=root/'src/main.rs'; text=main.read_text()
        assert 'mod coupled_status;' not in text
        text=text.replace('mod config;','mod config;\nmod coupled_status;',1)
        text=text.replace('    CoupledRun(tui::events::coupled_run::Args),','    CoupledRun(tui::events::coupled_run::Args),\n\n    /// Inspect saved coupled-run progress without executing work\n    CoupledStatus(coupled_status::StatusArgs),',1)
        anchor='        Some(Command::CoupledRun(args)) => tui::events::coupled_run::run(args, cli.common).await,'
        assert text.count(anchor)==1
        text=text.replace(anchor,anchor+'\n        Some(Command::CoupledStatus(args)) => coupled_status::run(args),',1)
        main.write_text(text); shutil.copyfile(HERE/'reference.rs',root/'src/coupled_status.rs')
        launcher=root/'npm/spiral-coder/lib/launcher.cjs'; text=launcher.read_text(); assert "'coupled-status'" not in text
        launcher.write_text(text.replace("'coupled-run',","'coupled-run', 'coupled-status',",1))
        sys.path.insert(0,str(HERE)); from status_fixture import expected
        (root/'docs/coupled-status.md').write_text('''# Inspect a long coupled run

Run `spiral coupled-status --run-dir ./run` (or `spiral-coder coupled-status --run-dir ./run`) to inspect existing records without API keys or file changes.
The following completed synthetic example is produced by `scripts.status_fixture.make`.

```json
'''+json.dumps(expected(),indent=2)+'''
```

Task usage counts task charges; wire usage counts every request including transport repairs and in-flight requests. Finalized and pending usage are counted once. A missing active/interrupted ledger produces null used and remaining totals, not zero. A malformed existing ledger fails with stderr and empty stdout. Native completion records work status; independent verification and promotion remain separate.
''')
        with (root/'docs/state-schema.md').open('a') as f:
            f.write('\n## Coupled status inspection\n\nThe read-only coupled-status projection lives in src/coupled_status.rs. It reads existing iterations.json and child state/tui-work.json ledgers; it owns no persisted state. Missing pending usage is null. Recorded completion is not independent verification.\n')
        shutil.copyfile(HERE/'reference_tests.rs',root/'tests/coupled_status.rs')
        subprocess.run(['cargo','fmt','--manifest-path',str(root/'Cargo.toml')],check=True)
        out.mkdir(parents=True)
        for name in ['luna-luna.json','prompt.txt',*(r['source'] for key in ('visible_fixtures','probes') for r in spec[key])]:
            target=out/name; target.parent.mkdir(parents=True,exist_ok=True)
            source = root/name.removeprefix('baseline/') if name.startswith('baseline/') else (HERE/'reference.rs' if name=='audit_reference.rs' else HERE/name)
            shutil.copy2(source,target)
        for r in spec['reference']:
            target=out/r['source']; target.parent.mkdir(parents=True,exist_ok=True); shutil.copy2(root/r['target'],target)
        (out/'baseline-ref.txt').write_text(base+'\n')

if __name__=='__main__':
    p=argparse.ArgumentParser(description=__doc__); p.add_argument('out',type=Path); p.add_argument('--baseline',required=True)
    a=p.parse_args(); materialize(a.out.resolve(),a.baseline)
