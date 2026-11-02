"""Separate format recovery from advice, execution and candidate qualification."""
import argparse
from collections import Counter
import hashlib
import json
from pathlib import Path
import sys

HERE = Path(__file__).resolve().parent
sys.path.insert(0, str(HERE.parents[2] / "scripts"))
from coupled_bench.common import read_json, sha
from self_dogfood.evidence import bound_run


def unique(pairs):
    fields = dict(pairs)
    if len(fields) != len(pairs):
        raise ValueError("ambiguous display object")
    return fields


def rejected_reply(message, issue):
    raw = message.get("raw_content") or message["content"]
    row = {"raw_sha256": hashlib.sha256(raw.encode()).hexdigest(), "issue": issue}
    try:
        first = json.loads(raw, object_pairs_hook=unique)["suggestions"][0]
        args = first["suggested_args"]
        if isinstance(args, dict):
            row["first_authored_suggestion"] = {"tool": first.get("suggested_tool"), "argument_keys": sorted(args)}
    except (ValueError, TypeError, KeyError, IndexError):
        pass  # A malformed reply supplies no reliable action description.
    return row


def observations(root, receipt):
    pin = read_json(HERE / "plan.json")["declaration_sha256"]
    if sha(root / "predeclared.json") != pin or not bound_run(root, pin, receipt):
        raise ValueError("wrong episode or receipt")
    assessment = read_json(root / "assessment-001.json")
    if (assessment["predeclaration_sha256"], assessment["run_receipt_sha256"]) != (pin, receipt):
        raise ValueError("unbound assessment")
    native = read_json(root / "episode/run/report.json")
    display = native["observer_display"]
    recoveries = []
    rejected = []
    assistant = None
    for index, msg in enumerate(display):
        if msg["role"] == "Assistant":
            assistant = msg
        if msg["role"] != "Tool":
            continue
        text = msg["content"]
        if text.startswith("(next-action) invalid response prevented automatic forwarding: "):
            issue = text.removeprefix("(next-action) invalid response prevented automatic forwarding: ").split("; raw Observer output", 1)[0]
            rejected.append(rejected_reply(assistant, issue))
        if text.startswith("(next-action) JSON envelope framing recovered: RepeatedIdenticalSummary;"):
            if assistant is None:
                raise ValueError("recovery with no preceding answer")
            raw = assistant.get("raw_content") or assistant["content"]
            recoveries.append({"observer_display_index": index, "raw_sha256": hashlib.sha256(raw.encode()).hexdigest(),
                               "passed_action_validation": assistant.get("raw_content") is not None})
    ledger = native["work"]["requests"]
    actions = native["execution"]["actions"]
    blocks = []
    for action in actions:
        result = action.get("result") or ""
        if action.get("executed") is False and result.startswith("GOVERNOR BLOCKED"):
            args = action.get("arguments") or {}
            blocks.append({"step": action["result_step"], "tool": action["tool"],
                           "path": args.get("path"), "command": args.get("command"),
                           "gate": next((line for line in result.splitlines() if line.startswith("[")), None)})
    return {"declaration_sha256": pin, "run_receipt_sha256": receipt,
            "native_completed": native["completed"], "termination": native["termination"],
            "wall_elapsed_ms": native["wall_elapsed_ms"], "role_requests": native["role_requests"],
            "wire_charges": dict(Counter(row["charge"] for row in ledger)),
            "identical_summary_recoveries": recoveries, "rejected_advice": rejected,
            "native_blocks": blocks,
            "cargo_inputs": [{"step": a["result_step"], "observation": a["cargo_inputs"]}
                             for a in actions if a.get("cargo_inputs") is not None],
            "observer_assists": [{"id": a["assist_id"], "continuation": a["continuation_id"], "decision": a["decision"],
                                  "suggestion_execution": a.get("suggestion_execution")} for a in native["work"]["assists"]],
            "qualified_candidate": assessment["qualified_candidate"]}


if __name__ == "__main__":
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("episode", type=Path)
    parser.add_argument("--receipt", required=True)
    args = parser.parse_args()
    print(json.dumps(observations(args.episode.resolve(), args.receipt), indent=2))
