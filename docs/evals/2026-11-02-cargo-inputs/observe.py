"""Extract native Cargo input records from the sealed episode. They are descriptive and do not score the task."""

import argparse
import hashlib
from pathlib import Path
import json
import sys

HERE = Path(__file__).resolve().parent
sys.path.insert(0, str(HERE.parents[2] / "scripts"))
from coupled_bench.common import read_json, sha
from self_dogfood.evidence import bound_run


def rejected_duplicates(display):
    """Describe repeated top-level strings in preserved replies without repairing or regrading them."""
    raw = ""
    rows = []
    for msg in display:
        if msg["role"] == "Assistant":
            raw = msg.get("raw_content") or msg.get("content") or ""
        elif (
            msg["role"] == "Tool"
            and "invalid response prevented automatic forwarding: duplicate JSON field:" in msg["content"]
        ):
            fields = {}
            for key, value in json.loads(raw, object_pairs_hook=list):
                fields.setdefault(key, []).append(value)
            rows.append(
                {
                    "raw_sha256": hashlib.sha256(raw.encode()).hexdigest(),
                    "repeated_fields": {
                        key: {
                            "count": len(values),
                            "identical_strings": all(v == values[0] for v in values)
                            if all(isinstance(v, str) for v in values)
                            else None,
                        }
                        for key, values in fields.items()
                        if len(values) > 1
                    },
                }
            )
    return rows


def observe(raw, receipt):
    pin = read_json(HERE / "plan.json")["declaration_sha256"]
    if sha(raw / "predeclared.json") != pin or not bound_run(raw, pin, receipt):
        raise ValueError("Episode does not match the retained declaration and receipt")
    assessment = read_json(raw / "assessment-001.json")
    if (assessment["predeclaration_sha256"], assessment["run_receipt_sha256"]) != (pin, receipt):
        raise ValueError("Independent assessment belongs to another episode")
    run = raw / "episode/run"
    report = read_json(run / "report.json")
    actions = report["execution"]["actions"]
    assists = report["work"]["assists"]
    rows = []
    for i, action in enumerate(actions):
        if action.get("cargo_inputs") is None:
            continue
        native = action["cargo_inputs"]
        rows.append(
            {
                "step": action["result_step"],
                "call_id": action["call_id"],
                "tool": action["tool"],
                "path": action["arguments"].get("path"),
                "command": action["arguments"].get("command"),
                "observation": native,
                "action_freshness_at_end": action["freshness"],
                "next_actions": [
                    {
                        "step": a["result_step"],
                        "tool": a["tool"],
                        "executed": a["executed"],
                        "path": a["arguments"].get("path"),
                        "command": a["arguments"].get("command"),
                    }
                    for a in actions[i + 1 : i + 4]
                ],
            }
        )
    visible = next(c for c in assessment["independent_checks"] if c["id"] == "catalog-visible")
    leaf = "src/tui_replay/catalog.rs"

    def steps(prefix):
        return [
            row["step"]
            for row in rows
            if row["observation"]["files"].get(leaf) == "listed"
            and (row["observation"]["target"] or "").startswith(prefix)
        ]

    return {
        "run_receipt_sha256": receipt,
        "native_cargo_observations": rows,
        "new_catalog_listed_by_product_at_steps": steps("bin "),
        "new_catalog_listed_by_test_target_at_steps": steps("unit tests "),
        "independent_cli_listing_check": {
            "id": visible["id"],
            "passed": visible["passed"],
            "exit_code": visible["exit_code"],
            "test_summary": visible["test_summary"],
        },
        "observer_decisions": [
            {
                "id": a["assist_id"],
                "decision": a["decision"],
                "tool": (a.get("suggestion_execution") or {}).get("tool"),
                "matching_call_ids": (a.get("suggestion_execution") or {}).get("matching_call_ids"),
            }
            for a in assists
        ],
        "native_completed": report["completed"],
        "post_run_duplicate_diagnosis": rejected_duplicates(report["observer_display"]),
        "termination": report["termination"],
        "qualified_candidate": assessment["qualified_candidate"],
    }


if __name__ == "__main__":
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("episode", type=Path)
    parser.add_argument("--receipt", required=True)
    args = parser.parse_args()
    print(json.dumps(observe(args.episode.resolve(), args.receipt), indent=2))
