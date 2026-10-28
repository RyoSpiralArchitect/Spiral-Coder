"""Separate native evidence, independent behavior, report fields and file scope."""
from collections import Counter
import re

from .common import inventory


def native_file_edit_changed(action):
    """New receipts distinguish operation success from a write. Missing outcome
    keeps legacy receipt semantics; null/unknown modern outcomes grant no edit.
    """
    return (action.get("tool") in ("write_file", "patch_file", "apply_diff")
            and action.get("executed") is True and action.get("succeeded") is True
            and action.get("edit_outcome", "changed") == "changed")


def assess(case, before, workspace, report, process, oracle):
    execution = report.get("execution") or {}
    actions = execution.get("actions", [])
    edits = [a for a in actions if native_file_edit_changed(a)
             and isinstance(a.get("result_step"), int)]
    first = min((a["result_step"] for a in edits), default=None)
    last = max((a["result_step"] for a in edits), default=None)
    # Include later shell mutations tracked by the runtime; file tools alone are insufficient.
    mutation = execution.get("mutation_step")
    if isinstance(mutation, int):
        last = max(last or 0, mutation)
    baselines, verified = {}, {}
    for check in case["checks"]:
        command = check["command"]
        calls = [a for a in actions if a.get("tool") == "exec"
                 and (a.get("arguments") or {}).get("command") == command
                 and a.get("executed") is True and isinstance(a.get("result_step"), int)]
        baselines[command] = not check["baseline_failure"] or (first is not None and any(
            a.get("succeeded") is False and a["result_step"] < first for a in calls))
        verified[command] = last is not None and any(a.get("succeeded") is True
                                                   and a["result_step"] > last for a in calls)
    done = [a for a in actions if a.get("tool") == "done" and a.get("executed") is True
            and a.get("result") == "OK: done"]
    attempts = [a for a in actions if a.get("tool") == "done"]
    args = ((done or attempts)[-1].get("arguments") or {}) if attempts else {}
    summary = args.get("summary", "")
    report_fields = {"all_artifacts_named": all(p in summary for p in case["editable"]),
                     "before_failed": bool(re.search(r"\bbefore\s*:\s*failed\b", summary, re.I)),
                     "after_passed": bool(re.search(r"\bafter\s*:\s*passed\b", summary, re.I)),
                     "baseline_commands_named": all(c["command"] in summary for c in case["checks"] if c["baseline_failure"])}
    after = inventory(workspace)
    changed = sorted(p for p in set(before) | set(after) if before.get(p) != after.get(p))
    allowed = set(case["editable"])
    other = [p for p in changed if p not in allowed]
    recorded_requests = (report.get("work") or {}).get("requests")
    requests = recorded_requests if isinstance(recorded_requests, list) else []
    native = {"native_edit": bool(edits), "baseline_before_edit": all(baselines.values()),
              "fresh_exact_checks": all(verified.values()),
              "no_pending_checks": execution.get("pending_exact_checks") == [],
              # The runtime's accepted done gate already validates exhaustive
              # completed/remaining coverage. Omission means an empty list there;
              # this default cannot rescue a rejected or absent native done.
              "accepted_done": bool(done),
              "no_remaining_acceptance": bool(done) and args.get("remaining_acceptance", []) == []}
    accepted = report.get("completed") is True
    core = (accepted and oracle["passed"] and all(native.values()) and all(report_fields.values())
            and process.get("exit_code") == 0 and not process.get("timed_out")
            and bool(requests) and len(requests) <= case["max_requests"])
    blocks = Counter()
    sequence = []
    for action in actions:
        result = action.get("result") or ""
        reasons = re.findall(r"\[([^\]\n]*(?:Gate|handoff|BLOCK)[^\]\n]*)\]", result, re.I)
        if result.startswith("GOVERNOR BLOCKED"):
            blocks.update(reasons or ["unspecified governor"])
        sequence.append({k: action.get(k) for k in ["call_id", "tool", "arguments", "executed", "succeeded", "edit_outcome", "result_step", "freshness"]}
                        | {"result_excerpt": result[:800]})
    return {"runtime_completed": accepted, "independent_artifacts_passed": oracle["passed"],
            "behavior_and_handoff_qualified": core, "strict_all_files_qualified": core and not other,
            "native": native, "native_contract_revision": "edit-outcome-v2",
            "completion_field_presence": {"remaining_acceptance": "remaining_acceptance" in args},
            "baseline_commands": baselines, "verified_commands": verified,
            "report_fields": report_fields, "authored_summary": summary,
            "changed_files": changed, "out_of_scope_files": other,
            "requests": len(requests) if isinstance(recorded_requests, list) else None,
            "request_roles": dict(Counter(r.get("role") for r in requests)),
            "wire_requests": requests, "assists": (report.get("work") or {}).get("assists", []),
            "blocks": dict(blocks), "actions": sequence, "oracle": oracle}
