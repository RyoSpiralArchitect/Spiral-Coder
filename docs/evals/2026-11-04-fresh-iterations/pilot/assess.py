"""Score an untouched fresh pilot against its sealed receipts and original independent oracle."""
import argparse
from collections import Counter
import hashlib
import json
from pathlib import Path
import sys

HERE = Path(__file__).resolve().parent
REPO = HERE.parents[3]
sys.path.insert(0, str(REPO / "scripts"))
from coupled_bench.common import inventory, read_json, sha
from coupled_bench.oracles import score


def assess(plan_file, trial, expected_receipt, binary):
    plan = read_json(plan_file)
    seal = read_json(trial / "receipt.json")
    assert sha(trial / "receipt.json") == expected_receipt and seal["plan_sha256"] == sha(plan_file)
    assert sha(binary) == plan["executor_sha256"] and inventory(trial / "run") == seal["run_inventory"]
    root = trial / "workspace"
    assert inventory(root) == seal["final_inventory"]
    fixture = REPO / plan["fixture"]
    assert inventory(fixture / "workspace") == plan["baseline_inventory"]
    assert sha(fixture / "task.txt") == sha(trial / "task.txt") == plan["prompt_sha256"]
    suite = REPO / "benchmarks/coding-v1"
    case = next(c for c in read_json(suite / "suite.json")["cases"] if c["id"] == "pilot-cli")
    check = case["checks"][0]["command"]
    original, final = plan["baseline_inventory"], seal["final_inventory"]
    changes = sorted(k for k in original.keys() | final.keys() if original.get(k) != final.get(k))
    manifest = read_json(trial / "run/iterations.json")
    seen, edits, checks, terminal, rounds = set(), [], [], [], []
    for i, item in enumerate(manifest["iterations"]):
        folder = trial / "run" / item["directory"]
        r = read_json(folder / "report.json")
        durable = read_json(folder / "state/tui-work.json")
        work = r["work"]
        assert work["requests"] == durable["requests"] and work["task_id"] == durable["task_id"] == item["task_id"]
        assert item["parent_task_id"] == (rounds[-1]["task_id"] if rounds else None)
        assert len(work["requests"]) == item["wire_requests"] and item["task_requests"] <= plan["limits"]["requests"]
        assert sum(x["charge"] == "task" for x in work["requests"]) == item["task_requests"]
        totals = Counter(x["role"] for x in work["requests"])
        assert r["role_requests"] == {role: totals[role] for role in ("coder", "observer")}
        assert work["root"] == (trial / "task.txt").read_text().strip() and work["tool_root"] == str(root)
        assert work["request_limit"] == plan["limits"]["requests"]
        assert work["policy"] == {"observer_reserved_requests": plan["limits"]["observer"],
                                  "transport_repair_limit": plan["limits"]["transport"]}
        assert r["roles"]["coder"]["model"] == r["roles"]["observer"]["model"] == plan["model"]
        messages = read_json(folder / "session.json")["messages"]
        outputs = {x["tool_call_id"]: x for x in messages if x.get("role") == "tool" and x.get("tool_call_id")}
        for x in r["execution"]["actions"]:
            call_id = x["call_id"]
            if call_id in seen or not x["executed"]:
                continue
            seen.add(call_id)
            step = x["result_step"]
            if x["edit_outcome"] == "changed":
                edits.append(dict(step=step, path=x["arguments"]["path"], call_id=call_id))
            elif x["tool"] == "exec" and x["arguments"].get("command") == check:
                checks.append(dict(step=step, command=check, success=x["succeeded"], call_id=call_id,
                                   counts=outputs.get(call_id, {}).get("test_counts")))
            elif x["tool"] == "done":
                terminal.append(dict(step=step, outcome=x["result"], args=x["arguments"]))
        rounds.append(dict(**item, roles=r["role_requests"], pending=r["execution"]["pending_exact_checks"],
                           observer_handoff=next((x["iteration_handoff"] for x in messages if "iteration_handoff" in x), None)))
    assert manifest["wire_requests"] == sum(x["wire_requests"] for x in rounds) <= plan["maximum_wire_requests"]
    assert manifest["task_requests"] == sum(x["task_requests"] for x in rounds) <= plan["maximum_task_requests"]
    oracle = score(case, suite, root, binary)
    for log in oracle["logs"]:
        log["sha256"] = hashlib.sha256(log.pop("output").encode()).hexdigest()
    accepted = [t for t in terminal if t["outcome"] == "OK: done"]
    red = any(x["success"] is False and (x["counts"] or {}).get("failure_events", 0) > 0 and
              (not edits or x["step"] < min(e["step"] for e in edits)) for x in checks)
    green = bool(edits and accepted and any(x["success"] is True and
                 max(e["step"] for e in edits) < x["step"] < accepted[-1]["step"] for x in checks))
    summary = accepted[-1]["args"].get("summary", "") if accepted else ""
    facts = bool(accepted and not accepted[-1]["args"].get("remaining_acceptance") and
                 all(s in summary for s in [*case["editable"], check, "Before: FAILED", "After: PASSED"]))
    process = seal["process"]
    both_roles = all(any(r["roles"][role] > 0 for r in rounds) for role in ("coder", "observer"))
    passed = bool(both_roles and manifest["completed"] and process["exit_code"] == 0 and not process["timed_out"]
                  and not process["output_limit"] and process["log_redactions"] == 0 and
                  set(changes) == set(case["editable"]) and red and green and facts and
                  not rounds[-1]["pending"] and oracle["passed"])
    return dict(plan_sha256=sha(plan_file), receipt_sha256=expected_receipt, process=process, manifest=manifest,
                rounds=rounds, changed_files=changes, edits=edits, checks=checks, reports=terminal,
                reproduced_original_failure=red, fresh_post_edit_pass=green, authored_closeout=facts,
                oracle=oracle, task_passed=passed, second_iteration_passed=passed and len(rounds) == 2)


if __name__ == "__main__":
    p = argparse.ArgumentParser(description=__doc__)
    for name in ("plan", "trial", "binary"):
        p.add_argument(name, type=Path)
    p.add_argument("receipt_sha256")
    args = p.parse_args()
    print(json.dumps(assess(args.plan, args.trial.resolve(), args.receipt_sha256, args.binary.resolve()), indent=2))
