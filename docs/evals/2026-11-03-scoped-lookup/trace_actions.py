"""Inspect native requests and tool actions in one sealed replay-catalog episode."""

import argparse
import json
from pathlib import Path
import sys

sys.path.insert(0, str(Path(__file__).resolve().parents[3] / "scripts"))
from coupled_bench.common import read_json, sha
from self_dogfood.evidence import bound_run


def measure(root, pin, receipt):
    if sha(root / "predeclared.json") != pin or not bound_run(root, pin, receipt):
        raise ValueError("unbound episode")
    run = root / "episode/run"
    trace = [
        json.loads(line) for line in (run / "trace.jsonl").read_bytes().splitlines()
    ]
    starts = [e["data"] for e in trace if e["event"] == "request_started"]
    batches = [
        e["data"]["tool_count"]
        for e in trace
        if e["event"] == "pre_tool_gate_rejected"
        and e["data"].get("gate") == "single_tool"
        and e["data"]["tool_count"] > 1
    ]
    report = read_json(run / "report.json")
    searches = []
    for action in report["execution"]["actions"]:
        if action["tool"] == "search_files":
            searches.append(
                {
                    k: action.get(k)
                    for k in ("result_step", "call_id", "arguments", "executed")
                }
            )
            searches[-1]["result_header"] = (action.get("result") or "").partition(
                "\n"
            )[0]
    return {
        "declaration_sha256": pin,
        "run_receipt_sha256": receipt,
        "role_requests": report["role_requests"],
        "wire_starts_in_trace": len(starts),
        "wire_starts_with_parallel_tool_calls_false": sum(
            e.get("parallel_tool_calls") is False for e in starts
        ),
        "multi_call_responses_rejected": batches,
        "searches": searches,
        "forced_edit_targets": [
            e["data"].get("next_target")
            for e in trace
            if e["event"] == "meta_harness_policy"
            and e["data"].get("action") == "mutate_existing_now"
        ],
    }


if __name__ == "__main__":
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("episode", type=Path)
    parser.add_argument("--declaration", required=True)
    parser.add_argument("--receipt", required=True)
    args = parser.parse_args()
    print(json.dumps(measure(args.episode, args.declaration, args.receipt), indent=2))
