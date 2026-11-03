"""Evaluate R13's two rejected reads, or score a new reply against the sealed session."""

import argparse
import hashlib
import json
from pathlib import Path
import sys

HERE = Path(__file__).resolve().parent
ROOT = HERE.parents[3]
sys.path.insert(0, str(ROOT / "scripts"))
from coupled_bench.common import read_json, sha
from self_dogfood.evidence import bound_run

sys.path.insert(0, str(HERE.parents[1] / "2026-11-03-deferred-edit"))
from replay_advice import native_evaluator

EXPECTED = {"path": "src/tui_replay.rs", "start_line": 150, "end_line": 349}


def run(args):
    raw = args.episode.resolve()
    receipt = read_json(HERE.parent / "results.json")["run_receipt_sha256"]
    assert bound_run(
        raw, read_json(HERE.parent / "plan.json")["declaration_sha256"], receipt
    )
    recording = raw / "episode/run"
    history = read_json(recording / "session.json")["messages"]
    report = read_json(recording / "report.json")
    rows = [
        m.get("raw_content") or m["content"]
        for m in report["observer_display"]
        if m["role"] == "Assistant"
    ]
    trace = [
        json.loads(line)
        for line in (recording / "trace.jsonl").read_bytes().splitlines()
    ]
    cfg = next(e["data"] for e in trace if e["event"] == "verification_config")
    original, retry = rows[-2:]
    rejected = dict(EXPECTED, end_line=350)
    assert all(
        json.loads(text)["suggestions"][0]["suggested_args"] == rejected
        for text in (original, retry)
    )
    packet = {
        "packet": {},
        "messages": history,
        "test_command": cfg["test_command"],
        "lang": "en",
        "reason": "next_action",
    }
    with native_evaluator(args.executor.resolve()) as evaluate:
        failures = [evaluate(dict(packet, response=text)) for text in (original, retry)]
        assert all(item["issue"] and not item["hint"] for item in failures)
        request = {
            "model": "gpt-5.6-luna",
            "messages": [
                {
                    "role": "system",
                    "content": "You are Observer in next-action assist mode. Focus on one concrete next step. Return the requested JSON.",
                },
                {"role": "user", "content": failures[0]["repair_prompt"]},
            ],
            "reasoning_effort": "none",
            "max_completion_tokens": 8192,
            "response_format": {"type": "json_object"},
            "stream": False,
        }
        body = json.dumps(request).encode()
        if args.request_out:
            args.request_out.write_bytes(body)
        result = {
            "source_run_receipt_sha256": receipt,
            "executor_sha256": sha(args.executor),
            "original_reply_sha256": hashlib.sha256(original.encode()).hexdigest(),
            "retry_reply_sha256": hashlib.sha256(retry.encode()).hexdigest(),
            "original_issue": failures[0]["issue"],
            "retry_issue": failures[1]["issue"],
            "original_forwarded": False,
            "retry_forwarded": False,
            "request_sha256": hashlib.sha256(body).hexdigest(),
        }
        if args.reply:
            actual = args.reply.read_text()
            verdict = evaluate(dict(packet, response=actual))
            suggestions = (verdict.get("envelope") or {}).get("suggestions") or []
            action = suggestions[0] if len(suggestions) == 1 else {}
            result.update(
                reply_sha256=hashlib.sha256(actual.encode()).hexdigest(),
                native_issue=verdict["issue"],
                suggested_args=action.get("suggested_args"),
                forwardable=bool(verdict.get("hint")),
                passed=not verdict["issue"]
                and bool(verdict.get("hint"))
                and action.get("suggested_tool") == "read_file"
                and action.get("suggested_args") == EXPECTED,
            )
    return result


if __name__ == "__main__":
    p = argparse.ArgumentParser(description=__doc__)
    p.add_argument("executor", type=Path)
    p.add_argument("episode", type=Path)
    p.add_argument("--request-out", type=Path)
    p.add_argument("--reply", type=Path)
    print(json.dumps(run(p.parse_args()), indent=2))
