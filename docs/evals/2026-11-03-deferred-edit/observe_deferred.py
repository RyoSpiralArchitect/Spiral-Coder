"""Re-evaluate retained native check prefixes. Eligibility is distinct from live delivery."""

import argparse
import json
from pathlib import Path
import sys

HERE = Path(__file__).resolve().parent
sys.path.insert(0, str(HERE.parents[2] / "scripts"))
sys.path.insert(0, str(HERE))
from coupled_bench.common import read_json, sha
from self_dogfood.evidence import bound_run
from replay_advice import digest, evaluator


def observe(root, receipt):
    plan = read_json(HERE / "plan.json")
    pin = plan["declaration_sha256"]
    executor = root / "runtime/spiral-coder"
    if (
        sha(root / "predeclared.json") != pin
        or sha(executor) != plan["executor_sha256"]
        or not bound_run(root, pin, receipt)
    ):
        raise ValueError("wrong episode, runtime or receipt")
    assessment = read_json(root / "assessment-001.json")
    if (assessment["predeclaration_sha256"], assessment["run_receipt_sha256"]) != (
        pin,
        receipt,
    ):
        raise ValueError("unbound independent assessment")
    run = root / "episode/run"
    native = read_json(run / "report.json")
    actions = native["execution"]["actions"]
    messages = read_json(run / "session.json")["messages"]
    positions = {}
    for i, message in enumerate(messages):
        if message.get("role") == "tool" and "tool_call_id" in message:
            call_id = message["tool_call_id"]
            if call_id in positions:
                raise ValueError("ambiguous native call ID")
            positions[call_id] = i + 1
    trace = list(map(json.loads, (run / "trace.jsonl").read_bytes().splitlines()))
    (config,) = [r["data"] for r in trace if r["event"] == "verification_config"]
    edit_hints = [
        {"t_ms": r["t_ms"], "target": r["data"].get("next_target")}
        for r in trace
        if r["event"] == "meta_harness_policy"
        and r["data"].get("action") == "mutate_existing_now"
    ]
    holds = [
        {
            "step": a["result_step"],
            "path": a["arguments"].get("path"),
            "call_id": a["call_id"],
        }
        for a in actions
        if a["tool"] == "apply_diff"
        and a.get("executed") is False
        and "[Recovery Gate] stage=verify"
        in (a.get("result") or "").split("\n\ntool:", 1)[0]
    ]
    successes = []
    offers = []
    with evaluator(executor) as evaluate:
        for a in actions:
            if a["tool"] != "exec" or a.get("succeeded") is not True:
                continue
            step = a["result_step"]
            proposal = evaluate(
                messages[: positions[a["call_id"]]], config["test_command"]
            )
            successes.append(
                {
                    "step": step,
                    "command": a["arguments"].get("command"),
                    "eligible": proposal is not None,
                }
            )
            if proposal:
                args = proposal["suggested_args"]
                (original,) = [
                    v for v in actions if v["call_id"] == proposal["proposal_call_id"]
                ]
                assert (
                    args == original["arguments"]
                    and a["call_id"] == proposal["verification_call_id"]
                )
                later = [
                    {
                        "step": v["result_step"],
                        "executed": v["executed"],
                        "edit_outcome": v.get("edit_outcome"),
                    }
                    for v in actions
                    if v.get("result_step")
                    and v["result_step"] > step
                    and v["tool"] == "apply_diff"
                    and v["arguments"] == args
                ]
                offers.append(
                    {
                        "step": step,
                        "original_step": original["result_step"],
                        "path": args["path"],
                        "diff_sha256": digest(args["diff"].encode()),
                        "later_equal_arguments": later,
                    }
                )
    return {
        "declaration_sha256": pin,
        "run_receipt_sha256": receipt,
        "retained_verification_holds": holds,
        "successful_exec_prefixes": successes,
        "retained_post_check_offers": offers,
        "forced_edit_targets": edit_hints,
        "role_requests": native["role_requests"],
        "termination": native["termination"],
        "completed": native["completed"],
        "qualified_candidate": assessment["qualified_candidate"],
        "post_run_provider_requests": 0,
    }


if __name__ == "__main__":
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("episode", type=Path)
    parser.add_argument("--receipt", required=True)
    args = parser.parse_args()
    print(json.dumps(observe(args.episode.resolve(), args.receipt), indent=2))
