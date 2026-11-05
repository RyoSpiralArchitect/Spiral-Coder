"""Check retained invalid replies and neighboring cases on the actual native evaluator."""
import argparse
import hashlib
import json
from pathlib import Path
import sys

HERE = Path(__file__).resolve().parent
SEALED = HERE.parent / "2026-11-03-scoped-lookup"
sys.path.insert(0, str(HERE.parent / "2026-11-03-deferred-edit"))
from replay_advice import native_evaluator


def digest(data):
    return hashlib.sha256(data).hexdigest()


def controls(binary, episode):
    manifest = json.loads((SEALED / "receipts.json").read_bytes())["files"]
    data = (episode / "episode/run/session.json").read_bytes()
    assert digest(data) == manifest["episode/run/session.json"]["sha256"]
    trace = (episode / "episode/run/trace.jsonl").read_bytes()
    assert digest(trace) == manifest["episode/run/trace.jsonl"]["sha256"]
    (config,) = [row["data"] for row in map(json.loads, trace.splitlines()) if row["event"] == "verification_config"]
    packet = {"packet": {}, "messages": json.loads(data)["messages"], "lang":"en",
              "reason":"verification_failure", "test_command":config["test_command"]}
    duplicate = (SEALED / "json-mode-reply.txt").read_text()
    assert digest(duplicate.encode()) == json.loads((SEALED/"json-mode-wire.json").read_bytes())["reply_sha256"]
    # Construct independent negative controls from an explicitly authored valid instance.
    def advice(path, confidence=.88):
        return {"summary":"Inspect the failure.", "response_contract":{"required":True,"focus_axes":["correctness"],"note":"Inspect first."},
                "suggestions":[{"kind":"read","reason":"inspect","confidence":confidence,"suggested_tool":"read_file","suggested_args":{"path":path},"based_on":["task_obligations"]}]}
    path = "src/tui_replay/catalog.rs"
    valid = json.dumps(advice(path),separators=(",", ":"))
    clarity = advice(path)
    clarity["suggestions"] = [{"kind":"clarify","reason":"Need more evidence.","suggested_tool":None,"suggested_args":{},"confidence":0.3}]
    cases = [
        ("original malformed", (SEALED/"observer-probe-reply.txt").read_text(), True, False),
        ("original identical contracts", duplicate, True, False),
        ("conflicting contracts", valid[:-1]+',"response_contract":{"required":false}}', True, False),
        ("duplicate contract member", valid.replace('"required":true','"required":true,"required":true'), True, False),
        ("duplicate typed action", valid.replace('"suggested_tool":"read_file"', '"suggested_tool":"read_file","suggested_tool":"exec"'), True, False),
        ("duplicate argument", valid.replace('"path":', '"path":"src/other.rs","path":'), True, False),
        ("one eligible read", valid, False, True),
        ("wrong diagnostic path", json.dumps(advice("src/tui_replay.rs")), True, False),
        ("low confidence", json.dumps(advice(path,.25)), False, False),
        ("uncertain no-tool clarification", json.dumps(clarity), False, False),
    ]
    rows=[]
    with native_evaluator(binary) as evaluate:
        for label, raw, invalid, forward in cases:
            res = evaluate(dict(packet,response=raw))
            assert res["packet"]["task_obligations"]["required_diagnostic_read"]["path"] == path
            assert (bool(res["issue"]), bool(res["hint"])) == (invalid,forward), label
            rows.append({"case":label,"reply_sha256":digest(raw.encode()),"issue":res["issue"],"forwardable":bool(res["hint"])})
    return {"executor_sha256":digest(binary.read_bytes()),"source_session_sha256":digest(data),"external_requests":0,"file_actions_executed":0,"cases":rows}


if __name__ == "__main__":
    parser=argparse.ArgumentParser(description=__doc__)
    parser.add_argument("binary",type=Path); parser.add_argument("r10_episode",type=Path)
    args=parser.parse_args()
    print(json.dumps(controls(args.binary.resolve(),args.r10_episode.resolve()),indent=2))
