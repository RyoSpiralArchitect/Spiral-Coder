"""Extra qualification for a same-model pair working on a real repository."""
from pathlib import Path
import re
import shutil
import subprocess
import tempfile

from coupled_bench.common import inventory
from coupled_bench.evidence import native_file_edit_changed
from coupled_bench.reporting import states_outcome


def python_summary(log):
    """Recognize a completed unittest run. Errors, skips and expected failures are not passes."""
    match = re.search(r"^Ran (\d+) tests? in [\d.]+s\s*\n(OK|FAILED \(failures=(\d+)\))\s*\Z", log, re.M)
    if not match:
        return None
    total, failed = int(match[1]), int(match[3] or 0)
    if total < failed:
        return None
    return ("FAILED" if failed else "ok", str(total - failed), str(failed), "0")


def patch_replay(out, after):
    """Reconstruct all files from the sealed patch, including modes and new files."""
    patch = out / "episode/candidate.patch"
    if not patch.is_file() or not patch.stat().st_size:
        return False
    with tempfile.TemporaryDirectory(dir=out / "scratch") as tmp:
        root = Path(tmp) / "workspace"
        shutil.copytree(out / "baseline", root)
        process = subprocess.run(["git", "apply", "--binary", "--whitespace=nowarn", str(patch)],
                                 cwd=root, capture_output=True, timeout=30)
        return process.returncode == 0 and inventory(root, include_nested_git=True) == after


def paired_gates(out, spec, report, before, after, changed):
    actions = (report.get("execution") or {}).get("actions", [])
    edits = [a["result_step"] for a in actions if native_file_edit_changed(a)
             and type(a.get("result_step")) is int]
    visible = next(c for c in spec["checks"] if c["id"] == spec["visible_check_id"])
    reproduced = any(
        a.get("tool") == "exec" and a.get("executed") is True and a.get("succeeded") is False
        and (a.get("arguments") or {}).get("command") == spec["reproducer_command"]
        and type(a.get("result_step")) is int and edits and a["result_step"] < min(edits)
        and all(marker in (a.get("result") or "") for marker in visible["baseline_failure_contains"])
        for a in actions)
    done = [a for a in actions if a.get("tool") == "done" and a.get("executed") is True
            and a.get("result") == "OK: done"]
    summary = ((done[-1].get("arguments") or {}).get("summary") if done else "") or ""
    summary = summary if isinstance(summary, str) else ""
    report_fields = {
        "changed_paths_named": bool(changed) and all(p in summary for p in changed),
        "commands_named": all(cmd in summary for cmd in spec["native_checks"]),
        "before_failed": states_outcome(summary, "Before", "FAILED"),
        "after_passed": states_outcome(summary, "After", "PASSED"),
    }
    requests = (report.get("work") or {}).get("requests") or []
    completed_roles = {r.get("role") for r in requests if r.get("status") == "completed"
                       and r.get("charge", "task") == "task"}
    regular = all(p in after and "sha256" in after[p] and
                  (after[p]["mode"] == before[p].get("mode") if p in before
                   else not after[p]["mode"] & 0o111) for p in changed)
    return ({"both_roles_completed_requests": {"coder", "observer"}.issubset(completed_roles),
             "native_reproduction": reproduced, "authored_report": all(report_fields.values()),
             "regular_source_changes": regular, "patch_replays_all_files": patch_replay(out, after)},
            {"report_fields": report_fields, "authored_summary": summary})
