"""Bounded operator-only test sensitivity audit on a disposable reference copy.

The candidate implementation is never changed or adopted. Four advertised faults
must change a control case and fail authored CLI tests that pass on the reference.
This is a limited sensitivity measure, not proof of comprehensive coverage.
"""
import argparse
import hashlib
import json
from pathlib import Path
import re
import shutil
import sys
import tempfile
import time

FAULTS = ('missing_pending', 'symlink', 'duplicate_request', 'parent_link')
KEY = 'SPIRAL_STATUS_AUDIT_FAULT'


def sha(path):
    return hashlib.sha256(path.read_bytes()).hexdigest()


def instrument(text):
    replacements = {
        '!meta.file_type().is_symlink()': '(audit_fault("symlink") || !meta.file_type().is_symlink())',
        'optional && e.kind()': 'optional && !audit_fault("missing_pending") && e.kind()',
        'ids.insert(r.id)': '(audit_fault("duplicate_request") || ids.insert(r.id))',
        'r.parent_task_id.as_deref() == parent &&': '(audit_fault("parent_link") || r.parent_task_id.as_deref() == parent) &&',
    }
    for before, after in replacements.items():
        if text.count(before) != 1:
            raise ValueError('Reference mutation anchor changed: ' + before)
        text = text.replace(before, after, 1)
    return text + '\nfn audit_fault(name: &str) -> bool { std::env::var("' + KEY + '").as_deref() == Ok(name) }\n'


def test_summary(log):
    matches = re.findall(r'test result: (ok|FAILED)\. (\d+) passed; (\d+) failed; (\d+) ignored;', log)
    if len(matches) != 1:
        return None
    status, passed, failed, ignored = matches[0]
    return dict(status=status, passed=int(passed), failed=int(failed), ignored=int(ignored))


def audit(prepared, tests, label, seconds):
    if not re.fullmatch(r'[a-z0-9-]+', label) or not 1 <= seconds <= 1200:
        raise ValueError('Invalid audit label or budget')
    declaration = json.loads((prepared / 'predeclared.json').read_bytes())
    for name, digest in declaration['driver_sha256'].items():
        if sha(prepared / 'driver' / name) != digest:
            raise ValueError('Frozen driver changed')
    sys.path.insert(0, str(prepared / 'driver'))
    from self_dogfood.contracts import load_frozen, apply_overlays, execution_env
    from self_dogfood.runner import cache_lock
    from self_dogfood.build_cache import reset_package_cache
    from self_dogfood.process import run_process
    declaration_sha = sha(prepared / 'predeclared.json')
    _, spec = load_frozen(prepared, declaration_sha)
    if tests.is_symlink() or not tests.is_file():
        raise ValueError('Authored tests must be a regular file')
    test_sha = sha(tests)
    destination = prepared / 'audits' / label
    destination.mkdir(parents=True, exist_ok=False)
    deadline = time.monotonic() + seconds
    target = Path(json.loads((prepared / 'layout.json').read_bytes())['target_dir'])
    raw_reference = prepared / 'inputs/audit_reference.rs'
    result = dict(version=1, provider_calls=0, audit_sha256=sha(Path(__file__)),
                  declaration_sha256=declaration_sha, tests_sha256=test_sha,
                  raw_reference_sha256=sha(raw_reference), wall_limit_seconds=seconds,
                  faults={}, passed=False, promotion_authorized=False)

    def run(argv, root, env, name):
        log = destination / (name + '.log')
        remaining = deadline - time.monotonic()
        if remaining <= 0:
            return dict(exit_code=None, timed_out=True, output_limit=False, test_summary=None)
        measured = run_process(argv, root, env, min(180, remaining), log)
        measured.update(log_sha256=sha(log), test_summary=test_summary(log.read_text(errors='replace')))
        return measured

    def succeeded(row):
        return row['exit_code'] == 0 and not row['timed_out'] and not row['output_limit']

    with cache_lock(target), tempfile.TemporaryDirectory(prefix='sensitivity-', dir=prepared / 'scratch') as tmp:
        root = Path(tmp) / 'workspace'
        shutil.copytree(prepared / 'baseline', root)
        apply_overlays(root, prepared / 'inputs', spec['reference'])
        shutil.copy2(tests, root / 'tests/coupled_status.rs')
        (root / 'src/coupled_status.rs').write_text(instrument(raw_reference.read_text()))
        result['instrumented_sha256'] = sha(root / 'src/coupled_status.rs')
        reset_package_cache(prepared, root, target, destination / 'cache-reset.json', min(60, max(.001, deadline-time.monotonic())))
        env = execution_env(prepared, target)
        env.pop(KEY, None)
        argv = ['cargo', 'test', '--locked', '--test', 'coupled_status', '--', '--test-threads=1']
        base = run(argv, root, env, 'reference-tests')
        result['reference_tests'] = base
        summary = base['test_summary']
        reference_pass = succeeded(base) and summary is not None and summary['passed'] > 0 and summary['failed'] == summary['ignored'] == 0
        result['reference_tests_passed'] = reference_pass
        binary = target / 'debug/spiral-coder'
        result['binary_sha256'] = sha(binary) if binary.exists() else None
        # Controls are created from the operator-owned fixture, never candidate code.
        import importlib.util
        module_spec = importlib.util.spec_from_file_location('audit_fixture', prepared / 'inputs/status_fixture.py')
        fixture = importlib.util.module_from_spec(module_spec); module_spec.loader.exec_module(fixture)
        for fault in FAULTS:
            case = Path(tmp) / fault
            m = fixture.make(case, 'unknown' if fault == 'missing_pending' else 'completed')
            ledger = case / 'iteration-01/state/tui-work.json'
            if fault == 'symlink':
                outside = Path(tmp) / 'outside-ledger.json'; ledger.rename(outside); ledger.symlink_to(outside)
            elif fault == 'duplicate_request':
                w = json.loads(ledger.read_bytes()); w['requests'][1]['id'] = w['requests'][0]['id']
                fixture.put(case, 'iteration-01/state/tui-work.json', w)
            elif fault == 'parent_link':
                m['iterations'][0]['parent_task_id'] = 'wrong-parent'; fixture.put(case, 'iterations.json', m)
            faulty_env = dict(env, **{KEY: fault})
            command = [str(binary), 'coupled-status', '--run-dir', str(case)]
            off = run(command, root, env, fault + '-control-off')
            on = run(command, root, faulty_env, fault + '-control-on')
            valid = fault == 'missing_pending'
            def control_matches(row, expected):
                if row['timed_out'] or row['output_limit'] or row['exit_code'] is None:
                    return False
                return (row['exit_code'] == 0) == expected
            control_pass = control_matches(off, valid) and control_matches(on, not valid)
            mutant = run(argv, root, faulty_env, fault + '-tests') if reference_pass and control_pass else None
            sm = mutant['test_summary'] if mutant else None
            killed = bool(mutant and mutant['exit_code'] != 0 and not mutant['timed_out'] and not mutant['output_limit']
                          and sm and sm['status'] == 'FAILED' and sm['failed'] > 0 and sm['ignored'] == 0
                          and sm['passed'] + sm['failed'] == summary['passed'])
            result['faults'][fault] = dict(control_off=off, control_on=on, control_demonstrated=control_pass,
                                          tests=mutant, killed_by_authored_tests=killed)
        result['tests_unchanged'] = sha(tests) == test_sha
        result['passed'] = bool(reference_pass and result['tests_unchanged'] and
                                all(r['control_demonstrated'] and r['killed_by_authored_tests'] for r in result['faults'].values()))
        load_frozen(prepared, declaration_sha)
    result['wall_seconds'] = round(seconds - (deadline - time.monotonic()), 3)
    (destination / 'result.json').write_text(json.dumps(result, indent=2) + '\n')
    return result


if __name__ == '__main__':
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--prepared', type=Path, required=True)
    parser.add_argument('--tests', type=Path, required=True)
    parser.add_argument('--label', required=True)
    parser.add_argument('--seconds', type=int, default=600)
    args = parser.parse_args()
    print(json.dumps(audit(args.prepared.resolve(), args.tests.resolve(), args.label, args.seconds), indent=2))
