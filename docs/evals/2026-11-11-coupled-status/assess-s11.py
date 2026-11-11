"""Append a prospectively pinned test-sensitivity gate to the sealed chain audit."""
import argparse,importlib.util,json
from pathlib import Path
import assess,live

def run(out,plan_path,receipt):
    plan=json.loads(plan_path.read_bytes())
    if live.digest(Path(__file__)) != plan['extended_assessor_sha256']: raise ValueError('Extended assessor changed')
    script=Path(__file__).with_name('test_sensitivity-s11.py')
    if live.digest(script) != plan['test_sensitivity']['script_sha256']: raise ValueError('Sensitivity assessor changed')
    result=assess.assess(out,plan_path,receipt)
    module_spec=importlib.util.spec_from_file_location('sensitivity',script)
    module=importlib.util.module_from_spec(module_spec);module_spec.loader.exec_module(module)
    tests=out/'episode/workspace/tests/coupled_status.rs'
    audit=module.audit(out,tests,'candidate',plan['test_sensitivity']['wall_seconds']) if tests.is_file() else {'passed':False,'reason':'required authored tests absent'}
    from self_dogfood.evidence import bound_run
    if not bound_run(out,plan['declaration_sha256'],receipt):raise ValueError('Candidate seal changed during sensitivity audit')
    result['test_sensitivity']=audit
    result['gates']['authored_test_sensitivity']=audit['passed']
    result['all_extended_gates_passed']=all(result['gates'].values())
    result['qualified_within_original_single_round']=result['all_extended_gates_passed'] and len(result['rounds'])==1
    return result

if __name__=='__main__':
    p=argparse.ArgumentParser(description=__doc__);p.add_argument('out',type=Path);p.add_argument('plan',type=Path);p.add_argument('receipt')
    args=p.parse_args();print(json.dumps(run(args.out.resolve(),args.plan.resolve(),args.receipt),indent=2))
