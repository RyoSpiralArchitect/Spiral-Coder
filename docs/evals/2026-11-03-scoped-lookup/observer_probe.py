"""Rebuild a repair request or score a supplied reply locally against sealed R10 evidence."""

import argparse
import hashlib
import json
from pathlib import Path
import sys

HERE = Path(__file__).resolve().parent
sys.path.insert(0, str(HERE.parent / "2026-11-03-deferred-edit"))
from replay_advice import native_evaluator


def sha(data):
    return hashlib.sha256(data).hexdigest()


def probe(args):
    files = json.loads((HERE / "receipts.json").read_bytes())["files"]

    def sealed(name):
        data = (args.episode / name).read_bytes()
        if sha(data) != files[name]["sha256"]:
            raise ValueError(f"wrong sealed artifact {name}")
        return data

    report = json.loads(sealed("episode/run/report.json"))
    messages = json.loads(sealed("episode/run/session.json"))["messages"]
    trace = map(json.loads, sealed("episode/run/trace.jsonl").splitlines())
    (config,) = [e["data"] for e in trace if e["event"] == "verification_config"]
    rows = [
        m.get("raw_content") or m["content"]
        for m in report["observer_display"]
        if m["role"] == "Assistant"
    ]
    original, rejected_retry = rows[-2:]
    packet = {
        "messages": messages,
        "packet": {},
        "lang": "en",
        "reason": "verification_failure",
        "test_command": config["test_command"],
    }
    with native_evaluator(args.executor.resolve()) as evaluate:
        first = evaluate(dict(packet, response=original))
        second = evaluate(dict(packet, response=rejected_retry))
        if not first["issue"] or not second["issue"] or first["hint"] or second["hint"]:
            raise ValueError("expected both retained replies to remain rejected")
        path = first["packet"]["task_obligations"]["required_diagnostic_read"]["path"]
        if path != "src/tui_replay/catalog.rs":
            raise ValueError("unexpected native diagnostic")
        system = (
            "You are Observer in next-action assist mode.\nDo not perform a broad critique.\n"
            "Focus only on the next concrete step.\n\n[Language]\nLanguage: English. Write the critique in English. "
            "Keep proposals block keys in English (title/to_coder/severity/score/phase/impact/cost)."
        )
        body = json.dumps(
            {
                "model": "gpt-5.6-luna",
                "messages": [
                    {"role": "system", "content": system},
                    {"role": "user", "content": first["repair_prompt"]},
                ],
                "reasoning_effort": "none",
                "max_completion_tokens": 8192,
                "stream": False,
            }
        ).encode()
        if args.request_out:
            args.request_out.write_bytes(body)
        result = {
            "executor_sha256": sha(args.executor.read_bytes()),
            "run_receipt_sha256": files["run-receipt.json"]["sha256"],
            "original_response_sha256": sha(original.encode()),
            "original_issue": first["issue"],
            "retry_response_sha256": sha(rejected_retry.encode()),
            "retry_issue": second["issue"],
            "required_path": path,
            "request_sha256": sha(body),
            "file_actions_executed": 0,
        }
        if args.reply:
            raw = args.reply.read_text()
            judged = evaluate(dict(packet, response=raw))
            suggestions = (judged["envelope"] or {}).get("suggestions") or []
            # A single grounded suggestion is the declared success criterion for this probe.
            action = suggestions[0] if len(suggestions) == 1 else {}
            result.update(
                reply_sha256=sha(raw.encode()),
                issue=judged["issue"],
                hint_forwardable=bool(judged["hint"]),
                suggested_tool=action.get("suggested_tool"),
                suggested_args=action.get("suggested_args"),
                passed=not judged["issue"]
                and bool(judged["hint"])
                and action.get("suggested_tool") == "read_file"
                and (action.get("suggested_args") or {}).get("path") == path,
            )
        return result


if __name__ == "__main__":
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("executor", type=Path)
    parser.add_argument("episode", type=Path)
    parser.add_argument("--request-out", type=Path)
    parser.add_argument("--reply", type=Path)
    print(json.dumps(probe(parser.parse_args()), indent=2))
