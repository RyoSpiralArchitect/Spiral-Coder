"""Independent offline CLI acceptance tests, outside every model workspace."""
import json
import os
from pathlib import Path
import shlex
import subprocess
import sys
import tempfile

def evaluate(workspace):
    script = Path(workspace) / 'scripts/run_coupled_pilot.py'
    results = {}
    with tempfile.TemporaryDirectory(prefix='spiral-dogfood-oracle-') as directory:
        root = Path(directory)
        repo = root / 'fixture'
        (repo / 'tests/fixtures').mkdir(parents=True)
        cases = []
        for name in ['alpha', 'beta', 'gamma']:
            (repo / name).mkdir()
            (repo / name / 'note.txt').write_text(name)
            cases.append({'id': name, 'tool_root': name, 'prompt': 'Inspect note.txt.', 'max_requests': 2})
        (repo / 'tests/fixtures/coupled-loop-v1.json').write_text(json.dumps({'version': 1, 'cases': cases}))
        fake = root / 'fake.py'
        fake.write_text('''import json, os, pathlib, sys
a=sys.argv[1:]
out=pathlib.Path(a[a.index('--out-dir')+1]); out.mkdir(parents=True)
case=json.loads(pathlib.Path(a[a.index('--case-spec')+1]).read_text())
with open(os.environ['DOGFOOD_ORACLE_CALLS'],'a') as f: f.write(case['id']+'\\n')
(out/'report.json').write_text(json.dumps({'completed':True,'execution':{'pending_exact_checks':[]},'work':{'requests':[]}}))
(out/'checks.json').write_text(json.dumps({'ok':True}))
''')
        binary = root / 'spiral-coder-fixture'
        binary.write_text('#!/bin/sh\nexec ' + shlex.quote(sys.executable) + ' -S ' + shlex.quote(str(fake)) + ' "$@"\n')
        binary.chmod(0o700)
        for name, selection, expected in [
            ('default', [], ['alpha', 'beta', 'gamma']),
            ('single', ['--case', 'beta'], ['beta']),
            ('ordered_unique', ['--case', 'gamma', '--case', 'alpha', '--case', 'gamma'], ['alpha', 'gamma']),
            ('unknown', ['--case', 'missing-case'], None),
        ]:
            out = root / name
            calls = root / (name + '.calls')
            env = {key: os.environ[key] for key in ['PATH', 'HOME', 'TMPDIR', 'LANG'] if key in os.environ}
            env['DOGFOOD_ORACLE_CALLS'] = str(calls)
            if expected is not None:
                env['OPENAI_API_KEY'] = 'offline-fixture-only'
            command = [sys.executable, '-S', str(script), '--binary', str(binary), '--repo', str(repo), '--out', str(out), '--provider', 'openai', '--model', 'offline', *selection]
            run = subprocess.run(command, env=env, text=True, capture_output=True, timeout=15)
            invoked = calls.read_text().splitlines() if calls.exists() else []
            checks = {'status': (run.returncode == 0) if expected is not None else (run.returncode != 0)}
            if expected is None:
                checks.update(no_output_directory=not out.exists(), no_subprocess=not invoked, error_identifies_case='missing-case' in run.stdout + run.stderr)
            else:
                checks['exact_execution_order'] = invoked == expected
                for file in ['metadata.json', 'summary.json']:
                    data = json.loads((out / file).read_text()) if (out / file).exists() else {}
                    checks[file + '_selection'] = data.get('selected_case_ids') == expected
                if (out / 'summary.json').exists():
                    checks['summary_cases'] = [case['id'] for case in json.loads((out / 'summary.json').read_text()).get('cases', [])] == expected
            results[name] = {'ok': all(checks.values()), 'checks': checks, 'invoked': invoked, 'returncode': run.returncode, 'stderr': run.stderr[-2000:]}
    return {'ok': all(item['ok'] for item in results.values()), 'cases': results}

if __name__ == '__main__':
    result = evaluate(sys.argv[1])
    print(json.dumps(result, indent=2))
    raise SystemExit(0 if result['ok'] else 1)
