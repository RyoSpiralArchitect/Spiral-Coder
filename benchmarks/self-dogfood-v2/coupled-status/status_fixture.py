"""Native-format fixtures shared by visible and independent status checks."""
import copy
import json
from pathlib import Path


def put(root, name, value):
    path = root / name
    path.parent.mkdir(parents=True, exist_ok=True)
    path.write_text(json.dumps(value, indent=2) + '\n')


def work(task, charges=('task', 'task', 'transport'), pending=False):
    # The template is emitted by WorkBudget's real durable writer. Preserve
    # every native field; scenario edits change values, not a guessed schema.
    native = json.loads(Path(__file__).with_name('native_work.json').read_text())
    result = copy.deepcopy(native)
    result['task_id'] = task
    result['tool_root'] = '/unused'
    result['requests'] = []
    for i, charge in enumerate(charges):
        receipt = copy.deepcopy(native['requests'][2 if charge == 'transport' else 0])
        receipt.update(id=i, role='coder' if i % 2 == 0 else 'observer', charge=charge,
                       status='in_flight_or_interrupted' if pending and i == len(charges)-1 else 'completed')
        if receipt['status'] == 'in_flight_or_interrupted': receipt['duration_ms'] = None
        result['requests'].append(receipt)
    return result



def make(root, state='completed'):
    row = {'iteration': 1, 'directory': 'iteration-01', 'task_id': 't1',
           'parent_task_id': None, 'termination': 'completed' if state == 'completed' else 'blocked',
           'completed': state == 'completed', 'task_requests': 2, 'wire_requests': 3,
           'progress': {'changed_files': ['src/a.rs'], 'newly_verified': ['cargo test -q'],
                        'newly_reproduced_baselines': [], 'user_refusal': False}}
    manifest = {'version': 1, 'max_iterations': 6, 'max_task_requests_per_iteration': 32,
                'max_task_requests': 192, 'max_wire_requests': 216,
                'completed': state == 'completed', 'stop_reason': 'completed' if state == 'completed' else None,
                'active_iteration': None, 'iterations': [row], 'task_requests': 2, 'wire_requests': 3}
    put(root, 'iteration-01/state/tui-work.json', work('t1'))
    if state in ('running', 'interrupted', 'unknown'):
        directory = 'iteration-02'
        if state == 'running':
            manifest['active_iteration'] = directory
        else:
            manifest['stop_reason'] = 'interrupted'
            manifest['interrupted_iteration'] = {'directory': directory,
                'task_requests': None if state == 'unknown' else 1,
                'wire_requests': None if state == 'unknown' else 2}
            manifest['task_requests'] = None if state == 'unknown' else 3
            manifest['wire_requests'] = None if state == 'unknown' else 5
        if state != 'unknown':
            put(root, directory + '/state/tui-work.json', work('t2', ('task', 'transport'), True))
    if state == 'stopped':
        manifest['stop_reason'] = 'user_refusal'
        row['progress']['user_refusal'] = True
    put(root, 'iterations.json', manifest)
    return manifest


def expected(state='completed'):
    pending = state in ('running', 'interrupted', 'unknown')
    known = state != 'unknown'
    return {'version': 1, 'status': 'completed' if state == 'completed' else ('running' if state == 'running' else 'stopped'),
            'stop_reason': 'completed' if state == 'completed' else (None if state == 'running' else ('user_refusal' if state == 'stopped' else 'interrupted')),
            'iterations': [{'iteration': 1, 'task_id': 't1',
                            'termination': 'completed' if state == 'completed' else 'blocked',
                            'task_requests': 2, 'wire_requests': 3,
                            'changed_files': ['src/a.rs'], 'newly_verified': ['cargo test -q']}],
            'pending': {'directory': 'iteration-02', 'task_id': 't2' if known else None,
                        'task_requests': 1 if known else None, 'wire_requests': 2 if known else None,
                        'in_flight_requests': 1 if known else None} if pending else None,
            'requests': {'task_used': (3 if pending else 2) if known else None,
                         'wire_used': (5 if pending else 3) if known else None,
                         'task_limit': 192, 'wire_limit': 216,
                         'task_remaining': (189 if pending else 190) if known else None,
                         'wire_remaining': (211 if pending else 213) if known else None}}


if __name__ == '__main__':
    import argparse
    parser = argparse.ArgumentParser(description='Write native-format status fixtures into a test directory.')
    parser.add_argument('root', type=Path)
    parser.add_argument('state', choices=('completed', 'running', 'interrupted', 'unknown', 'stopped'))
    args = parser.parse_args()
    make(args.root, args.state)
