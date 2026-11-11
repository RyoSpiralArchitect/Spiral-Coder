"""Read native iteration receipts without executing tools or contacting providers."""
from collections import Counter
import hashlib
import json
from pathlib import Path
import sys


def load(path):
    return json.loads(path.read_bytes())


def profile(run):
    manifest = load(run / "iterations.json")
    rounds, seen = [], set()
    all_tools, all_intervals, failures = Counter(), Counter(), Counter()
    for item in manifest["iterations"]:
        folder = run / item["directory"]
        report = load(folder / "report.json")
        requests = report["work"]["requests"]
        provider = Counter()
        for request in requests:
            provider[request["role"]] += request["duration_ms"] / 1000
        tools, rejected = Counter(), Counter()
        for action in report["execution"]["actions"]:
            if action["call_id"] in seen:
                continue
            seen.add(action["call_id"])
            tools[action["tool"]] += 1
            if action["executed"] is False:
                rejected[action["tool"]] += 1
            result = action.get("result") or ""
            for name, marker in [
                ("overlong_read", "read at most 200 lines per call"),
                ("ambiguous_diff", "old block not unique"),
                ("plan_gate", "[Plan Gate]"),
                ("recovery_gate", "[Recovery Gate]"),
                ("compile_failure", "could not compile"),
                ("report_feedback", "Missing from the final answer:"),
            ]:
                if marker in result:
                    failures[name] += 1
        trace_file = folder / "trace.jsonl"
        trace = [json.loads(line) for line in trace_file.read_text().splitlines()]
        intervals = Counter()
        for index, event in enumerate(trace):
            if event["event"] != "tool_call":
                continue
            end = next((r for r in trace[index + 1:] if r["event"] in
                        ("agent_iter", "agent_outcome", "agent_end")), None)
            if end is not None:
                intervals[event["data"]["name"]] += (end["t_ms"] - event["t_ms"]) / 1000
        all_tools.update(tools)
        all_intervals.update(intervals)
        rounds.append({
            "iteration": item["iteration"],
            "termination": report["termination"],
            "task_requests": item["task_requests"],
            "wire_requests": len(requests),
            "wall_seconds": report["wall_elapsed_ms"] / 1000,
            "provider_seconds": dict(provider),
            "new_tool_calls": dict(tools),
            "new_rejected_tool_calls": dict(rejected),
            "tool_interval_seconds": dict(intervals),
            "report_sha256": hashlib.sha256((folder / "report.json").read_bytes()).hexdigest(),
            "trace_sha256": hashlib.sha256(trace_file.read_bytes()).hexdigest(),
        })
    return {
        "manifest_sha256": hashlib.sha256((run / "iterations.json").read_bytes()).hexdigest(),
        "completed": manifest["completed"],
        "task_requests": manifest["task_requests"],
        "wire_requests": manifest["wire_requests"],
        "native_round_wall_seconds": sum(r["wall_seconds"] for r in rounds),
        "provider_seconds": sum(sum(r["provider_seconds"].values()) for r in rounds),
        "tool_calls": dict(all_tools),
        "observed_failure_markers": dict(failures),
        "tool_interval_seconds": dict(all_intervals),
        "rounds": rounds,
        "interpretation": "Provider durations are recorded request intervals, not token cost. Tool intervals span tool_call to the next agent iteration or outcome and include local overhead. They are timing estimates, not isolated CPU measurements. Tool calls and failure markers are deduplicated by call ID across retained iteration history; a result may contain multiple markers and pruned results may omit a marker. Qualification is assessed separately.",
    }


if __name__ == "__main__":
    print(json.dumps(profile(Path(sys.argv[1]).resolve()), indent=2))
