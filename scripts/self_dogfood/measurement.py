"""Development diagnostics only. These never replace binary qualification gates."""
import re
from coupled_bench.evidence import native_file_edit_changed


def budget_accounting(work, spec):
    policy = {"transport_repair_limit": spec.get("transport_repair_requests", 0),
              "observer_reserved_requests": spec.get("observer_reserved_requests", 0)}
    rows = work.get("requests")
    valid = isinstance(rows, list) and bool(rows) and all(isinstance(r, dict) for r in rows)
    rows = rows if valid else []
    valid = valid and work.get("request_limit") == spec["max_requests"]
    valid = valid and work.get("policy", {k: 0 for k in policy}) == policy
    task, transport, coder = 0, 0, 0
    for i, row in enumerate(rows):
        charge = row.get("charge", "task")
        valid = valid and row.get("id") == i and row.get("role") in ("coder", "observer")
        if charge == "task":
            task += 1
            coder += row.get("role") == "coder"
            valid = valid and row.get("transport_rejection") is None
        elif charge == "transport":
            transport += 1
            expected = "native_arguments" if row.get("role") == "coder" else "observer_envelope"
            valid = valid and row.get("status") == "completed" and row.get("transport_rejection") == expected
        else:
            valid = False
    valid = valid and 0 <= task <= spec["max_requests"] and transport <= policy["transport_repair_limit"]
    valid = valid and coder <= spec["max_requests"] - policy["observer_reserved_requests"]
    valid = valid and len(rows) <= spec["max_requests"] + policy["transport_repair_limit"]
    return {"valid": bool(valid), "wire_requests": len(rows), "task_requests": task,
            "transport_requests": transport, "coder_task_requests": coder, "policy": policy}


def development_progress(spec, report, independent, baseline, gates):
    actions = (report.get("execution") or {}).get("actions", [])
    edits = [a.get("result_step", 0) or 0 for a in actions if native_file_edit_changed(a)]
    command = spec.get("reproducer_command")
    visible_contract = next((c for c in spec.get("checks", []) if c["id"] == spec.get("visible_check_id")), {})
    markers = visible_contract.get("baseline_failure_contains", [])
    count = visible_contract.get("expected_test_count")
    failures = visible_contract.get("expected_baseline_failures", 1)

    def observed_failure(action):
        result = action.get("result") or ""
        summary = re.findall(r"test result: FAILED\. (\d+) passed; (\d+) failed; (\d+) ignored;", result)
        return (bool(markers) and count is not None and bool(summary)
                and summary[-1] == (str(count - failures), str(failures), "0")
                and all(marker in result for marker in markers))

    reproduced = None if command is None or not markers or count is None else any(
        a.get("tool") == "exec" and a.get("executed") is True and a.get("succeeded") is False
        and (a.get("arguments") or {}).get("command") == command
        and (not edits or (a.get("result_step") or 0) < min(edits)) and observed_failure(a) for a in actions)
    originals = {c["id"]: c for c in baseline}
    improvements = [c for c in independent if c["kind"] == "improvement"]
    behavior = None
    for check in improvements:
        before, after = originals.get(check["id"], {}).get("test_summary"), check.get("test_summary")
        if before is not None and after is not None:
            changed = after["passed"] > before["passed"] and after["failed"] < before["failed"]
            behavior = bool(behavior) or changed
    visible = next((c["passed"] for c in independent if c["id"] == spec.get("visible_check_id")), None)
    controls = [c for c in independent if c["kind"] == "regression"]
    stages = [
        ("reproduced", reproduced),
        ("behavior_changed", behavior),
        ("visible_passed", visible),
        ("control_non_regression", all(c["passed"] for c in controls) if controls else None),
        ("fresh_full_check", gates["fresh_native_checks"]),
        ("docs_and_receipts", all(gates[k] for k in ("required_paths_changed", "native_receipts_valid", "pinned_run_receipt", "accepted_done", "no_remaining_acceptance"))),
    ]
    contiguous = 0
    for _, passed in stages:
        if passed is not True:
            break
        contiguous += 1
    return {"development_only": True, "qualification_unchanged": True,
            "milestones": {name: "observed" if passed is True else "not_observed" if passed is False else "not_evaluated" for name, passed in stages},
            "highest_contiguous_stage": contiguous,
            "first_unmet_stage": stages[contiguous][0] if contiguous < len(stages) else None,
            "termination": report.get("termination", "legacy_unspecified")}
