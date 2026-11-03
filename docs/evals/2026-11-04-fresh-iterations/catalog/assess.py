"""Evaluate sealed native child ledgers as a single bounded catalog task."""
import argparse
from collections import Counter
import json
from pathlib import Path
from urllib.parse import urlsplit
import live


def assess(out, plan_path, receipt_digest):
    plan = json.loads(plan_path.read_bytes())
    driver = live.frozen_driver(out, plan)
    from coupled_bench.common import PROVIDERS, inventory, read_json, sha
    from self_dogfood.assessment import checks
    from self_dogfood.evidence import bound_run
    from self_dogfood.measurement import budget_accounting
    from self_dogfood.paired import paired_gates

    dec, spec = driver.load_frozen(out, plan["declaration_sha256"])
    assert bound_run(out, plan["declaration_sha256"], receipt_digest)
    episode = out / "episode"
    process = read_json(episode / "process.json")
    manifest = read_json(episode / "run/iterations.json")
    root = episode / "workspace"
    before, after = dec["baseline_inventory"], inventory(root, include_nested_git=True)
    changed = sorted(p for p in before.keys() | after.keys() if before.get(p) != after.get(p))
    later = read_json(episode / "candidate-inventory.json")
    expected_roles = {}
    for name, configured in spec["roles"].items():
        uri = urlsplit(PROVIDERS[configured["provider"]][1])
        expected_roles[name] = dict(**configured, transport="openai-compatible", endpoint_origin=f"{uri.scheme}://{uri.netloc}",
                                    temperature=spec["temperature"], max_tokens=spec["max_tokens"], timeout_seconds=spec["timeout_seconds"])
    expected_roles["observer"].update(temperature=0.2, max_tokens=spec["observer_max_tokens"])
    rows, prior, reports = [], None, []
    all_ledgers = True
    task = (episode / "task.txt").read_text().strip()
    for index, item in enumerate(manifest["iterations"], 1):
        assert item["directory"] == f"iteration-{index:02d}"
        child = episode / "run" / item["directory"]
        report = read_json(child / "report.json")
        work, durable = report["work"], read_json(child / "state/tui-work.json")
        counted = Counter(row["role"] for row in work["requests"])
        roles = {role: counted[role] for role in spec["roles"]}
        spend = budget_accounting(work, spec)
        ledger = (work == durable and work["task_id"] == item["task_id"] and report["parent_task_id"] == item["parent_task_id"] == prior
                  and work["root"] == task and work["tool_root"] == str(root) and report["role_requests"] == roles
                  and report["roles"] == expected_roles and report["same_model_both_roles"] is True
                  and report["coder_chunk_requests"] == spec["coder_chunk_requests"] and spend["valid"]
                  and spend["task_requests"] == item["task_requests"] and spend["wire_requests"] == item["wire_requests"]
                  and report["termination"] == item["termination"] and report["completed"] == item["completed"])
        rows.append(dict(**item, roles=roles, ledger_verified=ledger, budget=spend))
        all_ledgers &= ledger
        prior = work["task_id"]
        reports.append(report)
    assert rows
    final = reports[-1]
    actions = final["execution"]["actions"]
    all_declared = [*spec["native_checks"], *plan.get("additional_native_checks", [])]
    def ran_at_root(action):
        lines = (action.get("result") or "").splitlines()[1:3]
        if lines and lines[0].startswith("[history digest"):
            lines = lines[1:]
        return (lines and lines[0] == "cwd: " + str(root)) or (
            not any(line.startswith("cwd: ") for line in lines) and action["arguments"].get("cwd") == str(root))
    exact = {cmd: [dict(id=x["call_id"], step=x["result_step"]) for x in actions if x["tool"] == "exec" and
             x["arguments"].get("command") == cmd and x["executed"] and x["succeeded"] and x["freshness"] == "current" and
             ran_at_root(x) and x["result_step"] > (final["execution"]["mutation_step"] or 0)] for cmd in all_declared}
    done = [x for x in actions if x["tool"] == "done" and x["executed"] and x["result"] == "OK: done"]
    after_edits = bool(done and all(any(x["step"] < done[-1]["result_step"] for x in xs) for xs in exact.values()))
    pairs, details = paired_gates(out, spec, final, before, after, changed)
    completed_roles = {x["role"] for r in reports for x in r["work"]["requests"]
                       if x["status"] == "completed" and x["charge"] == "task"}
    pairs["both_roles_completed_requests"] = {"coder", "observer"} <= completed_roles
    # Full child reports retain a pre-edit reproduction that can disappear from the later message window.
    if not pairs["native_reproduction"] and len(reports) > 1:
        pairs["native_reproduction"] = paired_gates(out, spec, reports[0], before, after, changed)[0]["native_reproduction"]
    safe = all(not (root / p).is_symlink() and not any((root / part).is_symlink() for part in Path(p).parents)
               for p in spec["editable"])
    target = Path(read_json(out / "layout.json")["target_dir"])
    with driver.cache_lock(target):
        independent = checks(out, spec, "fresh-chain", candidate=root) if safe else []
    candidate_is_same = inventory(root, include_nested_git=True) == after
    driver.load_frozen(out, plan["declaration_sha256"])
    assert bound_run(out, plan["declaration_sha256"], receipt_digest)
    gates = dict(**pairs, sealed_input_and_output=True, sealed_candidate_matches=after == later,
        distinct_parent_linked_ledgers=all_ledgers and len({r["task_id"] for r in rows}) == len(rows),
        bounded_chain=(0 < len(rows) <= plan["iterations"] and
          sum(r["task_requests"] for r in rows) == manifest["task_requests"] <= plan["maximum_task_requests"] and
          sum(r["wire_requests"] for r in rows) == manifest["wire_requests"] <= plan["maximum_wire_requests"] and
          manifest["max_task_requests"] == plan["maximum_task_requests"] and manifest["max_wire_requests"] == plan["maximum_wire_requests"]),
        both_roles_called=all(any(r["roles"].get(k, 0) > 0 for r in rows) for k in ("coder", "observer")),
        native_completed=manifest["completed"] and final["completed"], accepted_done=bool(done),
        no_remaining_acceptance=bool(done and done[-1]["arguments"].get("remaining_acceptance") == []),
        native_edit=any(r["progress"]["changed_files"] for r in rows), actual_edit=bool(changed),
        required_paths_changed=set(spec["required_changed_paths"]) <= set(changed),
        fresh_native_checks=after_edits, no_pending_checks=final["execution"]["pending_exact_checks"] == [],
        strict_scope=set(changed) <= set(spec["editable"]),
        original_protected_files_preserved=all(before.get(p) == after.get(p) for p in dec["protected_paths"]),
        process_ok=process["exit_code"] == 0 and not process["timed_out"] and not process["output_limit"],
        no_secret_redactions=not process["redacted_files"] and process["log_redactions"] == 0,
        patch_retained=process["patch_exit_code"] == 0, candidate_unchanged_by_assessor=candidate_is_same,
        independent_checks_passed=len(independent) == len(spec["checks"]) and all(c["passed"] for c in independent))
    guide = root / "docs/evaluation.md"
    if plan.get("preserve_original_guide"):
        gates["original_guide_prefix_preserved"] = guide.read_bytes().startswith((out / "baseline/docs/evaluation.md").read_bytes())
    return dict(plan_sha256=sha(plan_path), declaration_sha256=plan["declaration_sha256"], sealed_receipt_sha256=receipt_digest,
        process=process, manifest=manifest, rounds=rows, gates=gates, all_chain_gates_passed=all(gates.values()),
        qualified_within_original_single_round=all(gates.values()) and len(rows)==1,
        promotion_authorized=False, changed_files=changed, patch_sha256=sha(episode / "candidate.patch"),
        exact_native_calls=exact, terminal_reports=[dict(step=a["result_step"], outcome=a["result"], args=a["arguments"])
        for a in actions if a["tool"]=="done"], report_fields=details["report_fields"],
        independent_checks=[{k:row.get(k) for k in ("id","passed","executed","skip_reason","exit_code","test_summary","log_sha256","candidate_binary_sha256")} for row in independent])


if __name__ == "__main__":
    p = argparse.ArgumentParser(description=__doc__)
    p.add_argument("out", type=Path)
    p.add_argument("plan", type=Path)
    p.add_argument("receipt")
    args = p.parse_args()
    print(json.dumps(assess(args.out.resolve(), args.plan.resolve(), args.receipt), indent=2))
