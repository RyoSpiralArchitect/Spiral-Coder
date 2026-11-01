"""Summarize one sealed catalog episode. Null outcomes stay unknown."""
import argparse
import hashlib
import json
from pathlib import Path
import re
import sys

REPO = Path(__file__).resolve().parents[4]
sys.path.insert(0, str(REPO / "scripts"))
from coupled_bench.common import read_json, sha
from coupled_bench.evidence import native_file_edit_changed
from self_dogfood.evidence import bound_run


def fields(row, *names):
    return {name: row.get(name) for name in names}


def measure(root, declaration, receipt):
    if sha(root / "predeclared.json") != declaration or not bound_run(root, declaration, receipt):
        raise ValueError("Declaration or episode does not match the retained pins")
    native = read_json(root / "episode/run/report.json")
    assessment = read_json(root / "assessment-001.json")
    if assessment["run_receipt_sha256"] != receipt or assessment["predeclaration_sha256"] != declaration:
        raise ValueError("Assessment belongs to a different run")
    actions = native["execution"]["actions"]
    by_call = {a["call_id"]: a for a in actions if a.get("call_id")}
    edits = []
    for action in actions:
        if native_file_edit_changed(action):
            statuses = re.findall(r"\[auto-test\]\s*[✓✗]?\s*(PASSED|FAILED)\b", action.get("result") or "")
            edits.append(dict(fields(action, "result_step", "tool", "call_id"),
                              path=(action.get("arguments") or {}).get("path"), auto_test=statuses[-1] if statuses else None))
    first = {}
    for row in edits:
        if row["path"]:
            first.setdefault(row["path"], row["result_step"])

    handoffs = []
    for assist in native["work"]["assists"]:
        suggested = assist.get("suggestion_execution") or {}
        linked = [by_call[i] for i in suggested.get("matching_call_ids", []) if i in by_call]
        handoffs.append({
            **fields(assist, "assist_id", "decision", "completed"),
            "tool": suggested.get("tool"), "path": (suggested.get("arguments") or {}).get("path"),
            "exact_execution": suggested.get("executed"), "reported_success": suggested.get("succeeded"),
            "matching_call_ids": suggested.get("matching_call_ids"),
            "linked_changed_calls": [a["call_id"] for a in linked if native_file_edit_changed(a)],
        })

    # Include incomplete/invalid reply text by digest; only direct JSON supplies these descriptive fields.
    replies = []
    for item in native["observer_display"]:
        if item.get("role") != "Assistant":
            continue
        raw = item.get("raw_content") or item.get("content") or ""
        try:
            parsed = json.loads(raw)
        except ValueError:
            parsed = None
        rows = parsed.get("suggestions") if isinstance(parsed, dict) else None
        primary = rows[0] if isinstance(rows, list) and rows and isinstance(rows[0], dict) else {}
        replies.append({"text_sha256": hashlib.sha256(raw.encode()).hexdigest(),
                        "first_listed_kind": primary.get("kind"), "first_listed_tool": primary.get("suggested_tool"),
                        "first_listed_path": (primary.get("suggested_args") or {}).get("path")
                        if isinstance(primary.get("suggested_args"), dict) else None})
    checks = [{**fields(c, "id", "passed", "exit_code", "test_summary", "log_sha256"),
               "has_process_result": c["exit_code"] is not None} for c in assessment["independent_checks"]]
    return {"predeclaration_sha256": declaration, "run_receipt_sha256": receipt,
            "assessment_sha256": sha(root / "assessment-001.json"),
            **fields(native, "completed", "termination", "role_requests", "report_closeout_attempted"),
            **fields(assessment, "qualified_candidate", "gates", "changed_files", "out_of_scope_files",
                     "authored_summary", "native_checks", "budget_accounting", "development_progress"),
            "first_changed_step_by_path": first, "changed_edits": edits, "observer_handoffs": handoffs,
            "observer_replies": replies, "independent_checks": checks,
            "process": fields(assessment["process"], "exit_code", "timed_out", "wall_seconds")}


if __name__ == "__main__":
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("episode", type=Path)
    parser.add_argument("--declaration", required=True)
    parser.add_argument("--receipt", required=True)
    args = parser.parse_args()
    print(json.dumps(measure(args.episode, args.declaration, args.receipt), indent=2))
