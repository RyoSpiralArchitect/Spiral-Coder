"""Replay two proposed checks on the exact H2 source, stopping before a third reply."""
from http.server import BaseHTTPRequestHandler, ThreadingHTTPServer
import argparse
import json
from pathlib import Path
import re
import shutil
import subprocess
import sys
import tempfile
import threading

HERE = Path(__file__).resolve().parent
sys.path.insert(0, str(HERE.parents[3] / "scripts"))
from coupled_bench.common import inventory, read_json, sha
from self_dogfood.contracts import execution_env
from self_dogfood.evidence import bound_run

FMT = "cargo fmt --all -- --check"


def replay(raw, binary):
    evidence = HERE.parent / "h2"
    pin = read_json(evidence / "plan.json")["declaration_sha256"]
    receipt = read_json(evidence / "results.json")["run_receipt_sha256"]
    assert bound_run(raw, pin, receipt)
    native = raw / "episode/run"
    actions = {a["result_step"]: a for a in read_json(native / "report.json")["execution"]["actions"]}
    assert actions[12]["executed"] and actions[12]["succeeded"]
    assert all(a.get("edit_outcome") != "changed" for n, a in actions.items() if n > 12)
    seed = read_json(native / "session.json")
    messages = seed["messages"]
    end = next(i + 1 for i, m in enumerate(messages) if m.get("tool_call_id") == actions[12]["call_id"])
    seed["messages"] = messages[:end]
    admitted = next(m["accepted_plan"] for m in reversed(seed["messages"]) if isinstance(m.get("accepted_plan"), dict))
    later = next(i for i, m in enumerate(messages) if m.get("tool_call_id") == actions[18]["call_id"])
    proposal = messages[later - 1]["content"]
    assert "[Plan Gate] Invalid <plan>" in actions[18]["result"] and actions[18]["arguments"]["command"] == FMT
    corrected, n1 = re.subn(r"(?m)^goal: [^\n]+", lambda _: "goal: " + admitted["goal"], proposal, count=1)
    numbered = " ".join(f"{i}) {text}" for i, text in enumerate(admitted["acceptance_criteria"], 1))
    corrected, n2 = re.subn(r"(?m)^acceptance: [^\n]+", lambda _: "acceptance: " + numbered, corrected, count=1)
    assert n1 == n2 == 1 and corrected != proposal
    sent = []

    class Handler(BaseHTTPRequestHandler):
        def log_message(self, *_):
            pass

        def do_POST(self):
            self.rfile.read(int(self.headers["Content-Length"]))
            if len(sent) == 2:
                self.send_response(400)
                self.end_headers()
                self.wfile.write(b"fixture complete")
                return
            i = len(sent)
            call = {"index": 0, "id": f"replay-plan-{i}", "type": "function",
                    "function": {"name": "exec", "arguments": '{"check_id":"required-2"}'}}
            delta = {"choices": [{"delta": {"content": (proposal, corrected)[i], "tool_calls": [call]},
                                  "finish_reason": "tool_calls"}]}
            response = ("data: " + json.dumps(delta) + "\n\ndata: [DONE]\n\n").encode()
            sent.append(call["id"])
            self.send_response(200)
            self.send_header("Content-Type", "text/event-stream")
            self.send_header("Content-Length", str(len(response)))
            self.end_headers()
            self.wfile.write(response)

    with tempfile.TemporaryDirectory() as tmp, ThreadingHTTPServer(("127.0.0.1", 0), Handler) as server:
        scratch = Path(tmp).resolve()
        root, original = scratch / "source", raw / "episode/workspace"
        shutil.copytree(original, root, ignore=shutil.ignore_patterns(".git"))
        (scratch / "seed.json").write_text(json.dumps(seed).replace(str(original), str(root)))
        before = inventory(root)
        thread = threading.Thread(target=server.serve_forever, daemon=True)
        thread.start()
        try:
            env = execution_env(scratch, scratch / "target")
            subprocess.run([binary, "--provider", "openai", "--model", "fixture", "--base-url",
                            f"http://127.0.0.1:{server.server_port}", "--api-key", "fixture", "coupled-run",
                            "--without-observer", "--tool-root", root, "--prompt-file", raw / "episode/task.txt",
                            "--seed-session", scratch / "seed.json", "--out-dir", scratch / "run", "--max-requests", "8"],
                           env=env, cwd=root, capture_output=True, timeout=60)
        finally:
            server.shutdown()
            thread.join()
        assert before == inventory(root) and len(sent) == 2
        observed = [a for a in read_json(scratch / "run/report.json")["execution"]["actions"] if a["call_id"] in sent]
        assert len(observed) == 2 and all(a["arguments"].get("command") == FMT for a in observed)
        result = []
        for a in observed:
            output = a["result"]
            result.append({"id": a["call_id"], "executed": a["executed"], "succeeded": a["succeeded"],
                           "plan_refused": "[Plan Gate] Invalid <plan>" in output,
                           "quoted_objective": admitted["goal"] in output,
                           "recovery_refused": "[Recovery Gate]" in output,
                           "rustfmt_reported_diff": "Diff in " in output})
    return {"executor_sha256": sha(binary), "source_run_receipt_sha256": receipt, "protocol": "H2 prefix through 12; step 18 proposal twice, second with admitted goal and ordered acceptance",
            "external_requests": 0, "workspace_unchanged": True, "actions": result}


if __name__ == "__main__":
    p = argparse.ArgumentParser(description=__doc__)
    p.add_argument("raw", type=Path)
    p.add_argument("binary", type=Path)
    args = p.parse_args()
    print(json.dumps(replay(args.raw.resolve(), args.binary.resolve()), indent=2))
