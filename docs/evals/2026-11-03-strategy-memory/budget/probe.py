"""Rebuild the last R17 Observer decision from its sealed history and ledger."""

import argparse
import hashlib
import json
from pathlib import Path
import sys

HERE = Path(__file__).resolve().parent
sys.path.insert(0, str(HERE.parents[3] / "scripts"))
from coupled_bench.common import read_json, sha
from self_dogfood.evidence import bound_run

sys.path.insert(0, str(HERE.parents[1] / "2026-11-03-deferred-edit"))
from replay_advice import native_evaluator


def digest(data):
    return hashlib.sha256(data).hexdigest()


def run(args):
    raw = args.episode.resolve()
    source = read_json(HERE.parent / "results.json")
    assert bound_run(raw, source["predeclaration_sha256"], source["run_receipt_sha256"])
    recording = raw / "episode/run"
    report = read_json(recording / "report.json")
    messages = read_json(recording / "session.json")["messages"]
    assert messages[-1]["tool_call_id"] == report["execution"]["actions"][-1]["call_id"]
    assert messages[-1]["content"].startswith("GOVERNOR BLOCKED\n[Blocked report]")
    work = report["work"]
    previous, last = work["requests"][:-1], work["requests"][-1]
    assert last["role"] == "observer" and last["charge"] == "task"
    task = [entry for entry in previous if entry["charge"] == "task"]
    remaining = work["request_limit"] - len(task)
    coder = work["request_limit"] - work["policy"]["observer_reserved_requests"] - sum(
        entry["role"] == "coder" for entry in task
    )
    assert (remaining, coder) == (1, 0)
    budget = dict(limit=work["request_limit"], used=len(previous), remaining=remaining, coder_remaining=coder)
    original = next(
        m.get("raw_content") or m["content"] for m in reversed(report["observer_display"])
        if m["role"] == "Assistant"
    )
    trace = (json.loads(line) for line in (recording / "trace.jsonl").read_bytes().splitlines())
    config = next(e["data"] for e in trace if e["event"] == "verification_config")
    request = dict(packet={"work_budget": budget}, messages=messages, lang="en",
                   test_command=config["test_command"], reason="next_action")
    with native_evaluator(args.executor.resolve()) as evaluate:
        before = evaluate(dict(request, response=original))
        prompt = evaluate(request)["prompt"]
        system = ("You are Observer in next-action assist mode.\n"
                  "Consider the task objective, retained history, outstanding evidence and remaining requests.\n"
                  "Recommend one feasible next step or a handoff for a future continuation.\n\n"
                  "[Language]\nLanguage: English. Write the critique in English. Keep proposals block keys "
                  "in English (title/to_coder/severity/score/phase/impact/cost).")
        body = json.dumps(dict(model="gpt-5.6-luna", messages=[dict(role="system", content=system),
            dict(role="user", content=prompt)], reasoning_effort="none", max_completion_tokens=1200,
            response_format={"type":"json_object"}, stream=False)).encode()
        if args.request_out:
            args.request_out.write_bytes(body)
        result = dict(source_run_receipt_sha256=source["run_receipt_sha256"],
                      executor_sha256=sha(args.executor), pre_send_budget=budget,
                      native_view=before["packet"]["work_budget"],
                      original_reply_sha256=digest(original.encode()),
                      original_rejection=before["issue"], native_user_prompt_sha256=digest(prompt.encode()),
                      request_sha256=digest(body))
        if args.reply:
            text = args.reply.read_text()
            scored = evaluate(dict(request, response=text))
            envelope = scored["envelope"] or {}
            actions = envelope.get("suggestions") or []
            result.update(reply_sha256=digest(text.encode()), reply=json.loads(text) if envelope else text,
                          native_issue=scored["issue"], queued_hint=bool(scored["hint"]),
                          queued_response_contract=bool(scored["response_contract"]),
                          valid_no_tool_handoff=bool(envelope and scored["issue"] is None and
                            len(actions) == 1 and actions[0]["kind"] == "clarify" and
                            actions[0]["suggested_tool"] is None and actions[0]["suggested_args"] == {} and
                            not envelope["response_contract"]["required"] and not scored["hint"]))
    return result


if __name__ == "__main__":
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("executor", type=Path)
    parser.add_argument("episode", type=Path)
    parser.add_argument("--request-out", type=Path)
    parser.add_argument("--reply", type=Path)
    print(json.dumps(run(parser.parse_args()), indent=2))
