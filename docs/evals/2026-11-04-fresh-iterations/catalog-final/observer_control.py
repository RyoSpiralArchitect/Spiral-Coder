"""Re-evaluate both sealed terminal replies against the same saved native Coder context."""
import hashlib
import json
from pathlib import Path
import sys

HERE = Path(__file__).resolve().parent
REPO = HERE.parents[3]
sys.path.insert(0,str(REPO/"scripts"))
from coupled_bench.common import read_json, sha
sys.path.insert(0,str(HERE.parents[1]/"2026-11-03-deferred-edit"))
from replay_advice import native_evaluator


def review(original, baseline, fixed):
    trial=original/"episode/run/iteration-01"
    seal=read_json(original/"run-receipt.json")["episode_inventory"]
    assert all(sha(trial/f)==seal["run/iteration-01/"+f]["sha256"] for f in ("report.json","session.json","trace.jsonl"))
    r=read_json(trial/"report.json")
    assert r["termination"]=="observer_unavailable"
    replies=[m.get("raw_content") or m["content"] for m in r["observer_display"] if m["role"]=="Assistant"][-2:]
    assert replies==read_json(REPO/"tests/fixtures/observer-handoff/single-reference.json")
    trace=[json.loads(line) for line in (trial/"trace.jsonl").read_text().splitlines()]
    check=next(t["data"].get("test_command") for t in trace if t["event"]=="verification_config")
    packet=dict(packet={"work_budget":{"remaining":2,"coder_remaining":2}},messages=read_json(trial/"session.json")["messages"],
                test_command=check,lang="en",reason="incomplete_task",repair_native=True)
    rows=[]
    for label,exe in (("old",baseline),("revised",fixed)):
        with native_evaluator(exe) as evaluate:
            for raw in replies:
                response=evaluate(dict(packet,response=raw))
                rows.append(dict(runtime=label,executor_sha256=sha(exe),reply_sha256=hashlib.sha256(raw.encode()).hexdigest(),
                                 issue=response["issue"],hint_available=bool(response["hint"]),
                                 next_turn=response["packet"]["work_budget"].get("next_coder_turn")))
    assert all(r["next_turn"] == "report_only" and (r["issue"] is None) == (r["runtime"] == "revised")
               and r["hint_available"] == (r["runtime"] == "revised") for r in rows), rows
    return dict(seal_sha256=sha(original/"run-receipt.json"),session_sha256=sha(trial/"session.json"),rows=rows,
                budget_context="Two shared and two Coder requests before the Observer send; admitting the answer projects one Coder report-only seat.",
                external_requests=0,synthetic_tool_executions=0,automatically_completed=False)


if __name__=="__main__":
    print(json.dumps(review(*(Path(x).resolve() for x in sys.argv[1:])),indent=2))
