"""Bind native evidence to one externally pinned, completed runner receipt."""
from pathlib import Path
import re
import json

from coupled_bench.common import inventory, read_json, sha, write_json
from .contracts import now


def seal_run(out, declaration_digest):
    receipt = {"version": 1, "finished_at": now(), "predeclaration_sha256": declaration_digest,
               "episode_inventory": inventory(out / "episode")}
    write_json(out / "run-receipt.json", receipt)
    return sha(out / "run-receipt.json")


def bound_run(out, declaration_digest, run_digest):
    # Missing receipts still permit inspection of an interrupted durable ledger,
    # but can never qualify a candidate. A supplied incorrect pin is an error.
    if not run_digest:
        return False
    path = out / "run-receipt.json"
    if not re.fullmatch(r"[0-9a-f]{64}", run_digest) or sha(path) != run_digest:
        raise ValueError("Run receipt does not match the operator-pinned digest")
    receipt = read_json(path)
    if receipt["predeclaration_sha256"] != declaration_digest or inventory(out / "episode") != receipt["episode_inventory"]:
        raise ValueError("Episode artifacts changed after the pinned run receipt")
    return True


def native_evidence(out):
    """Retain interrupted accounting even if another receipt was only partly written."""
    errors = []

    def problem(path, reason):
        errors.append({"path": path, "reason": reason})

    def object_file(name):
        path = out / name
        try:
            value = read_json(path)
        except FileNotFoundError:
            problem(name, "missing")
            return None
        except (OSError, ValueError) as error:
            # Error kinds/positions are useful; never reproduce native file content.
            detail = f"JSONDecodeError at line {error.lineno} column {error.colno}" if isinstance(error, json.JSONDecodeError) else type(error).__name__
            problem(name, detail)
            return None
        if not isinstance(value, dict):
            problem(name, "expected JSON object")
            return None
        return value

    report_path = "episode/run/report.json"
    ledger_path = "episode/run/state/tui-work.json"
    report = object_file(report_path)
    durable = object_file(ledger_path)
    process = object_file("episode/process.json") or {}
    work = report.get("work") if report is not None else None
    source = "report"
    if not isinstance(work, dict):
        if report is not None:
            problem(report_path, "work must be an object")
        work = durable or {}
        source = "durable_ledger" if durable is not None else "unavailable"
    report = report or {"completed": False}
    report["work"] = work
    requests = work.get("requests")
    if requests is not None and (not isinstance(requests, list) or any(not isinstance(row, dict) for row in requests)):
        problem(report_path if source == "report" else ledger_path, "requests must be an array of objects")
        work["requests"] = None
    execution = report.get("execution")
    if execution is not None and not isinstance(execution, dict):
        problem(report_path, "execution must be an object")
        execution = None
    execution = execution or {}
    actions = execution.get("actions", [])
    if not isinstance(actions, list) or any(not isinstance(row, dict) for row in actions):
        problem(report_path, "actions must be an array of objects")
        actions = []
    for action in actions:
        if action.get("arguments") is not None and not isinstance(action["arguments"], dict):
            problem(report_path, "action arguments must be an object")
            action["arguments"] = {}
        if action.get("result_step") is not None and type(action["result_step"]) is not int:
            problem(report_path, "action result_step must be an integer")
            action["result_step"] = None
    if execution.get("mutation_step") is not None and type(execution["mutation_step"]) is not int:
        problem(report_path, "mutation_step must be an integer")
        execution["mutation_step"] = None
    execution["actions"] = actions
    report["execution"] = execution
    return report, durable, process, {"source": source, "errors": errors}
