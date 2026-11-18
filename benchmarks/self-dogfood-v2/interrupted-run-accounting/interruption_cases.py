"""Compare the public status CLI with native writer states around an interruption."""
import argparse
from copy import deepcopy
import hashlib
import importlib.util
import json
from pathlib import Path
import subprocess
import tempfile


def load_fixture(file):
    spec = importlib.util.spec_from_file_location("status_fixture", file)
    fixture = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(fixture)
    return fixture


def project(fixture, finalized=(), pending=None, state="interrupted", per=32, rounds=6, wire_limit=None):
    """Build native-format records independently of the candidate's parser."""
    with tempfile.TemporaryDirectory() as directory:
        manifest = fixture.make(Path(directory))
    template = manifest["iterations"][0]
    limit = per * rounds
    if wire_limit is None:
        wire_limit = limit + rounds * 4
    manifest.update(max_iterations=rounds, max_task_requests_per_iteration=per,
                    max_task_requests=limit, max_wire_requests=wire_limit,
                    iterations=[], completed=False, stop_reason=None, active_iteration=None)
    rows = []
    task_used = wire_used = 0
    ledgers = {}
    for n, charges in enumerate(finalized, 1):
        directory = f"iteration-{n:02}"
        name = f"t{n}"
        ledger = fixture.work(name, charges)
        ledgers[directory] = ledger
        row = dict(deepcopy(template), iteration=n, directory=directory, task_id=name,
                   parent_task_id=f"t{n-1}" if n > 1 else None, termination="blocked",
                   completed=False, task_requests=charges.count("task"), wire_requests=len(charges))
        task_used += row["task_requests"]
        wire_used += row["wire_requests"]
        manifest["iterations"].append(row)
        output_row = {k: row[k] for k in ("iteration", "task_id", "termination", "task_requests", "wire_requests")}
        output_row.update({k: row["progress"][k] for k in ("changed_files", "newly_verified")})
        rows.append(output_row)
    manifest.update(task_requests=task_used, wire_requests=wire_used)
    view = None
    if state != "start":
        n = len(finalized) + 1
        directory, name = f"iteration-{n:02}", f"t{n}"
        if pending is None:
            t = w = flight = None
        else:
            ledger = fixture.work(name, pending, pending=True)
            ledgers[directory] = ledger
            t, w = pending.count("task"), len(pending)
            flight = sum(r["status"] == "in_flight_or_interrupted" for r in ledger["requests"])
        view = dict(directory=directory, task_id=None if pending is None else name,
                    task_requests=t, wire_requests=w, in_flight_requests=flight)
        task_used = None if t is None else task_used + t
        wire_used = None if w is None else wire_used + w
        if state == "active":
            manifest["active_iteration"] = directory
        else:
            manifest.update(stop_reason="interrupted", interrupted_iteration=dict(directory=directory, task_requests=t, wire_requests=w),
                            task_requests=task_used, wire_requests=wire_used)
    usage = dict(task_used=task_used, wire_used=wire_used, task_limit=limit, wire_limit=wire_limit,
                 task_remaining=None if task_used is None else limit-task_used,
                 wire_remaining=None if wire_used is None else wire_limit-wire_used)
    expected = dict(version=1, status="stopped" if state == "interrupted" else "running",
                    iterations=rows, pending=view, requests=usage, stop_reason=manifest["stop_reason"])
    return manifest, ledgers, expected


def cases():
    prior = (("task", "transport"),)
    known = dict(finalized=prior, pending=("task", "transport"))
    unknown = dict(finalized=prior)
    yield "empty_start", dict(state="start"), None
    for state in ("active", "interrupted"):
        for name, pending in (("missing", None), ("empty", ()), ("spent", ("task", "transport"))):
            yield f"first_{state}_{name}", dict(state=state, pending=pending), None
    yield "after_previous_unknown", unknown, None
    yield "after_previous_known", known, None
    yield "known_at_each_cap", dict(finalized=(("task",),), pending=("task",), per=1, rounds=2, wire_limit=2), None
    for key in ("task_requests", "wire_requests"):
        yield "known_manifest_null_"+key, known, ("manifest", key, None)
        yield "missing_manifest_numeric_"+key, unknown, ("manifest", key, 0)
        yield "missing_interruption_numeric_"+key, unknown, ("interrupted_iteration", key, 1)
        yield "known_interruption_null_"+key, known, ("interrupted_iteration", key, None)
    yield "both_manifest_totals_lost", known, ("manifest", "both", None)
    for state in ("active", "interrupted"):
        limits = dict(state=state, rounds=2, per=1, wire_limit=2)
        yield f"finalized_at_cap_{state}_unknown", dict(limits, finalized=(("transport", "transport"),)), None
        yield f"finalized_over_cap_{state}_unknown", dict(limits, finalized=(("transport",)*3,)), ("invalid",)
        yield f"pending_round_over_cap_{state}", dict(state=state, finalized=(("task",),), pending=("task", "task"), rounds=3, per=1), ("invalid",)
    yield "unknown_charge_in_pending", dict(state="active", pending=("transport",)), ("charge","unrecognized")
    yield "omitted_native_charge", dict(state="active", pending=("task",)), ("charge",None)


def run(binary, fixture_file):
    fixture = load_fixture(fixture_file)
    results = {}
    for name, config, mutation in cases():
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            manifest, ledgers, expected = project(fixture, **config)
            valid = mutation is None or mutation == ("charge",None)
            if mutation and mutation[0] in ("manifest","interrupted_iteration"):
                group, key, value = mutation
                record = manifest if group == "manifest" else manifest[group]
                for field in (("task_requests","wire_requests") if key == "both" else (key,)):
                    record[field] = value
            elif mutation and mutation[0] == "charge":
                request = list(ledgers.values())[-1]["requests"][0]
                if mutation[1] is None:
                    del request["charge"]
                else:
                    request["charge"] = mutation[1]
            fixture.put(root, "iterations.json", manifest)
            for key, ledger in ledgers.items():
                fixture.put(root, key+"/state/tui-work.json", ledger)
            p = subprocess.run([binary,"coupled-status","--run-dir",root],capture_output=True,text=True,timeout=15)
            if valid:
                try:
                    actual = json.loads(p.stdout)
                except (ValueError, TypeError):
                    actual = None
                passed = p.returncode == 0 and actual == expected
            else:
                passed = p.returncode != 0 and p.stdout == "" and bool(p.stderr.strip())
            results[name] = {"expected_success":valid, "passed":passed, "exit":p.returncode,
                             "stdout":p.stdout.strip(), "stderr":p.stderr.strip()}
    digest = lambda path: hashlib.sha256(path.read_bytes()).hexdigest()
    return {"binary_sha256":digest(binary), "fixture_sha256":digest(fixture_file),
            "native_template_sha256":digest(fixture_file.with_name("native_work.json")),
            "probe_sha256":digest(Path(__file__)), "passed":sum(r["passed"] for r in results.values()),
            "total":len(results),"cases":results}


if __name__ == "__main__":
    parser=argparse.ArgumentParser(description=__doc__)
    parser.add_argument("binary",type=Path)
    parser.add_argument("fixture",type=Path)
    args=parser.parse_args()
    result=run(args.binary.resolve(),args.fixture.resolve())
    print(json.dumps(result, indent=2))
    raise SystemExit(int(result["passed"] != result["total"]))
