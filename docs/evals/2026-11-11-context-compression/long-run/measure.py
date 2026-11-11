"""Measure input size and transcript retention from finalized native rounds."""
from collections import Counter
import json
from pathlib import Path
from statistics import median
import sys

sys.path.insert(0, str(Path(__file__).resolve().parents[4] / "scripts"))
from coupled_bench.common import read_json, sha
from coupled_bench.roles import configuration_evidence


def request_summary(requests):
    sizes = [row["estimated_input_tokens"] for row in requests]
    return {
        "wire_requests": len(requests),
        "task_requests": sum(row["charge"] == "task" for row in requests),
        "statuses": dict(Counter(row["status"] for row in requests)),
        "estimated_input_tokens_total": sum(sizes),
        "estimated_input_tokens_median": median(sizes) if sizes else None,
        "estimated_input_tokens_max": max(sizes, default=None),
        "provider_usage_receipts": sum(row["provider_usage"] is not None for row in requests),
        "provider_seconds": sum(row["duration_ms"] for row in requests) / 1000,
    }


def compaction_summary(events):
    views = [row["data"] for row in events if row["event"] == "context_compaction"]
    before = sum(row["before_bytes"] for row in views)
    after = sum(row["after_bytes"] for row in views)
    reduced = [row for row in views if row["after_bytes"] < row["before_bytes"]]
    return {
        "assemblies": len(views),
        "reduced_assemblies": len(reduced),
        "protected_overflows": sum(row["protected_overflow"] for row in views),
        "message_bytes_before_total": before,
        "message_bytes_after_total": after,
        "message_bytes_saved": before - after,
        "message_bytes_before_max": max((row["before_bytes"] for row in views), default=None),
        "message_bytes_after_max": max((row["after_bytes"] for row in views), default=None),
        "largest_compacted_exchange_count": max((row["compacted_exchanges"] for row in views), default=None),
    }


def tool_messages(session):
    values = {"call_messages": Counter(), "result_messages": Counter()}
    for message in session["messages"]:
        role = message.get("role")
        if role == "assistant" and message.get("tool_calls"):
            kind = "call_messages"
        elif role == "tool":
            kind = "result_messages"
        else:
            continue
        values[kind][json.dumps(message, sort_keys=True)] += 1
    return values


def retention(previous, current):
    return {
        kind: {
            "previous": previous[kind].total(),
            "retained_exactly": (previous[kind] & messages).total(),
            "current": messages.total(),
        }
        for kind, messages in current.items()
    }


def measure(out):
    run = out / "episode" / "run"
    manifest = read_json(run / "iterations.json")
    spec = read_json(out / "inputs" / "spec.json")
    profile = dict(spec["roles"]["coder"], observer=spec["roles"]["observer"])
    prior_messages = prior_task = None
    all_requests, rounds = [], []
    for item in manifest["iterations"]:
        folder = run / item["directory"]
        report = read_json(folder / "report.json")
        work = report["work"]
        requests = work["requests"]
        all_requests.extend(requests)
        trace = folder / "trace.jsonl"
        events = [json.loads(line) for line in trace.read_text().splitlines()]
        rejections = {
            (row["data"]["cursor"], row["data"]["role"], row["data"]["reason"])
            for row in events
            if row["event"] == "transport_rejection"
        }
        session = folder / "state" / "tui-session.json"
        native = read_json(session)
        messages = tool_messages(native)
        config = configuration_evidence(report, profile, spec, spec)
        rounds.append({
            "iteration": item["iteration"],
            "termination": report["termination"],
            "native_wall_seconds": report["wall_elapsed_ms"] / 1000,
            "by_role": {
                role: request_summary([row for row in requests if row["role"] == role])
                for role in spec["roles"]
            },
            "coder_context": compaction_summary(events),
            "durable_work_matches": work == read_json(folder / "state" / "tui-work.json"),
            "parent_link_matches": report["parent_task_id"] == item["parent_task_id"] == prior_task,
            "current_configuration_checks": config["checks"],
            "transport_rejections": [
                {
                    "request_id": row["id"],
                    "role": row["role"],
                    "reason": row["transport_rejection"],
                    "correlated_trace": (row["id"], row["role"], row["transport_rejection"]) in rejections,
                }
                for row in requests
                if row["charge"] == "transport"
            ],
            "native_messages": len(native["messages"]),
            "native_session_bytes": session.stat().st_size,
            "tool_message_retention": retention(prior_messages, messages) if prior_messages is not None else None,
            "pending_exact_checks": len(report["execution"]["pending_exact_checks"]),
            "session_sha256": sha(session),
            "report_sha256": sha(folder / "report.json"),
            "trace_sha256": sha(trace),
        })
        prior_task, prior_messages = item["task_id"], messages
    return {
        "sealed_receipt_sha256": sha(out / "run-receipt.json"),
        "manifest_sha256": sha(run / "iterations.json"),
        "completed": manifest["completed"],
        "active_iteration_at_stop": manifest["active_iteration"],
        "by_role": {
            role: request_summary([row for row in all_requests if row["role"] == role])
            for role in spec["roles"]
        },
        "rounds": rounds,
        "method": (
            "Finalized native rounds only. Token estimates are serialized payload characters divided "
            "by four, rounded up. Coder byte telemetry measures message assembly before and after "
            "history projection and excludes tool schemas. Cross-round retention compares full tool "
            "message JSON values as multisets. Current configuration checks describe the executed "
            "ledger separately from the frozen assessor's verdict."
        ),
    }


if __name__ == "__main__":
    print(json.dumps(measure(Path(sys.argv[1]).resolve()), indent=2))
