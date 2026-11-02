"""Measure retained navigation, native file reads and actual CLI behavior in the sealed R8 episode."""
import argparse
import json
from pathlib import Path
import sys

HERE = Path(__file__).resolve().parent
sys.path.insert(0, str(HERE.parents[2] / "scripts"))
from coupled_bench.common import read_json, sha
from self_dogfood.evidence import bound_run

MARKER = "[navigation] Related Rust spelling "
MAIN = "src/main.rs"
ANCHORS = {
    "sibling_variant": "TuiReplay(TuiReplayArgs),",
    "sibling_dispatch": "Some(Command::TuiReplay(args)) => tui_replay::run(",
}


def summarize_action(action):
    args = action.get("arguments") or {}
    return {"step": action.get("result_step"), "tool": action["tool"],
            "executed": action.get("executed"),
            **{k: args[k] for k in ("dir", "pattern", "path", "start_line", "end_line", "check_id", "command") if k in args}}


def observe(root, receipt):
    pin = read_json(HERE / "plan.json")["declaration_sha256"]
    if sha(root / "predeclared.json") != pin or not bound_run(root, pin, receipt):
        raise ValueError("unbound native episode")
    assessment = read_json(root / "assessment-001.json")
    if (assessment["predeclaration_sha256"], assessment["run_receipt_sha256"]) != (pin, receipt):
        raise ValueError("unbound independent assessment")
    main = root / "baseline" / MAIN
    assert sha(main) == read_json(root / "predeclared.json")["baseline_inventory"][MAIN]["sha256"]
    lines = main.read_text().splitlines()
    baseline_locations = {}
    for name, snippet in ANCHORS.items():
        n, = [n for n, line in enumerate(lines, 1) if snippet in line]
        baseline_locations[name] = n
    report = read_json(root / "episode/run/report.json")
    session = read_json(root / "episode/run/session.json")
    actions = report["execution"]["actions"]
    results = {m["tool_call_id"]: m.get("content", "") for m in session["messages"]
               if m.get("role") == "tool" and "tool_call_id" in m}
    searches, main_reads = [], []
    for index, action in enumerate(actions):
        args = action.get("arguments") or {}
        text = results.get(action["call_id"], action.get("result") or "")
        if action["tool"] == "search_files":
            notes = [line for line in text.splitlines() if line.startswith(MARKER)]
            rows = {"action": summarize_action(action), "retained_notes": notes,
                    "header": text.splitlines()[0][:200] if text else None}
            if notes:
                rows["next_native_actions"] = [summarize_action(a) for a in actions[index + 1:index + 5]]
            searches.append(rows)
        if (action["tool"] == "read_file" and action.get("executed") is True and args.get("path") == MAIN
                and text.startswith(f"[{MAIN}]")):
            first, last = args.get("start_line"), args.get("end_line")
            covered = ([name for name, n in baseline_locations.items() if first <= n <= last]
                       if isinstance(first, int) and isinstance(last, int) else None)
            main_reads.append({"action": summarize_action(action), "requested_range_covers_baseline_anchors": covered,
                               "anchors_in_retained_text": [name for name, snippet in ANCHORS.items() if snippet in text]})
    return {"declaration_sha256": pin, "run_receipt_sha256": receipt, "baseline_locations": baseline_locations,
            "searches": searches, "main_source_reads": main_reads,
            "retained_navigation_count": sum(len(row["retained_notes"]) for row in searches),
            "retained_sibling_variant_and_dispatch": all(any(name in row["anchors_in_retained_text"] for row in main_reads)
                                                        for name in ANCHORS),
            "native_completed": report["completed"], "termination": report["termination"],
            "role_requests": report["role_requests"], "wall_elapsed_ms": report["wall_elapsed_ms"],
            "qualified_candidate": assessment["qualified_candidate"]}


if __name__ == "__main__":
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("episode", type=Path)
    parser.add_argument("--receipt", required=True)
    args = parser.parse_args()
    print(json.dumps(observe(args.episode.resolve(), args.receipt), indent=2))
