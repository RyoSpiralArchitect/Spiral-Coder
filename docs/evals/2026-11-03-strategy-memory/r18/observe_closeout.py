"""Trace the guide, exact checks, terminal report and remaining Observer budget."""
import argparse
import hashlib
import json
from pathlib import Path
import sys

DOCS = Path(__file__).resolve().parents[3]
sys.path.insert(0, str(DOCS.parent / "scripts"))
from coupled_bench.common import read_json, sha
from coupled_bench.evidence import native_file_edit_changed
from self_dogfood.evidence import bound_run

sys.path.insert(0, str(DOCS / "evals/2026-11-03-diagnostic-read"))
from trace_handoffs import unique


def observer_decisions(report, trace):
    work = report["work"]
    attempts = work["requests"]
    starts = [e["data"] for e in trace if e["event"] == "request_started"]
    calls = [r for r in attempts if r["role"] == "observer"]
    replies = [m for m in report["observer_display"] if m["role"] == "Assistant"]
    aligned = (
        len(starts) == len(attempts) and len(calls) == len(replies)
        and all(r["id"] == i and r["estimated_input_tokens"] == starts[i].get("estimated_input_tokens")
                for i, r in enumerate(attempts))
        and all(r["status"] in {"completed", "output_limited"} for r in calls)
    )
    shared = work["request_limit"]
    coder_slots = shared - work["policy"]["observer_reserved_requests"]
    turns = []
    for r in attempts:
        if r["role"] == "observer":
            turn = {"wire": r["id"] + 1, "charge": r["charge"], "status": r["status"],
                    "shared_before": shared, "coder_before": min(shared, coder_slots),
                    "coder_after_one_task_charge": min(max(0, shared - 1), coder_slots)}
            if aligned:
                message = replies[len(turns)]
                raw = message.get("raw_content") or message["content"]
                try:
                    reply = json.loads(raw, object_pairs_hook=unique)
                except ValueError:
                    reply = None
                envelope = reply if isinstance(reply, dict) else {}
                rows = envelope.get("suggestions")
                primary = rows[0] if isinstance(rows, list) and rows and isinstance(rows[0], dict) else {}
                args = primary.get("suggested_args")
                args = args if isinstance(args, dict) else {}
                turn.update(reply_sha256=hashlib.sha256(raw.encode()).hexdigest(),
                            native_validated=message.get("raw_content") is not None,
                            summary=envelope.get("summary"), why_this_first=envelope.get("why_this_first"),
                            reason=primary.get("reason"), based_on=primary.get("based_on"),
                            kind=primary.get("kind"), tool=primary.get("suggested_tool"),
                            path=args.get("path"), command=args.get("command"))
            turns.append(turn)
        if r["charge"] == "task":
            shared -= 1
            coder_slots -= int(r["role"] == "coder")
    return {"replies_aligned": aligned, "decisions": turns}


def measure(root, pin, receipt):
    if sha(root / "predeclared.json") != pin or not bound_run(root, pin, receipt):
        raise ValueError("unbound episode")
    run = root / "episode/run"
    report = read_json(run / "report.json")
    trace = [json.loads(line) for line in (run / "trace.jsonl").read_bytes().splitlines()]
    assessment = read_json(root / "assessment-001.json")
    if assessment["run_receipt_sha256"] != receipt or assessment["predeclaration_sha256"] != pin:
        raise ValueError("unbound assessment")
    execution = report["execution"]
    actions = execution["actions"]
    edits = [a for a in actions if native_file_edit_changed(a)]
    guide = "docs/evaluation.md"
    guide_steps = [a["result_step"] for a in edits if a["arguments"].get("path") == guide]
    mutations = [a["result_step"] for a in edits]
    if execution.get("mutation_step") is not None:
        mutations.append(execution["mutation_step"])
    last_mutation = max(mutations, default=None)
    checks = {}
    for command in read_json(root / "inputs/spec.json")["native_checks"]:
        checks[command] = [{k: a.get(k) for k in ("result_step", "call_id", "executed", "succeeded", "freshness")}
                           for a in actions if a["tool"] == "exec" and a["arguments"].get("command") == command]
    current = {command: [r["result_step"] for r in rows if r["executed"] and r["succeeded"]
                         and r["freshness"] == "current" and last_mutation is not None
                         and r["result_step"] > last_mutation] for command, rows in checks.items()}
    done = [a["result_step"] for a in actions if a["tool"] == "done" and a.get("executed")
            and a.get("result") == "OK: done"]
    ordered = bool(guide_steps and done and all(any(step < done[-1] for step in v) for v in current.values())
                   and guide in assessment["changed_files"] and report["completed"]
                   and not execution["pending_exact_checks"])
    return {"declaration_sha256": pin, "run_receipt_sha256": receipt,
            "guide_in_sealed_diff": guide in assessment["changed_files"], "native_guide_edit_steps": guide_steps,
            "last_native_mutation_step": last_mutation, "exact_command_attempts": checks,
            "fresh_passes_after_final_mutation": current, "accepted_successful_done_steps": done,
            "pending_checks": execution["pending_exact_checks"], "guide_then_checks_then_report": ordered,
            "qualified_candidate": assessment["qualified_candidate"],
            "budget_note": "Remaining counts are reconstructed from native ledger charges. A repair can reuse a prior prompt. Budget-compatible advice alone does not establish explicit reasoning about the count.",
            **observer_decisions(report, trace)}


if __name__ == "__main__":
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("episode", type=Path)
    parser.add_argument("--declaration", required=True)
    parser.add_argument("--receipt", required=True)
    args = parser.parse_args()
    print(json.dumps(measure(args.episode, args.declaration, args.receipt), indent=2, ensure_ascii=False))
