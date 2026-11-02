"""Send R7's original file-scoped search through the native loop and capture both role requests."""
import argparse
import hashlib
from http.server import BaseHTTPRequestHandler, ThreadingHTTPServer
import json
import os
from pathlib import Path
import subprocess
import tempfile
import threading

HERE = Path(__file__).resolve().parent
PRIOR = HERE.parent / "2026-11-02-observer-summary"
CALL = "route-replay-search"
MARKER = "[navigation]"


def digest(data):
    return hashlib.sha256(data).hexdigest()


def replay(executor, episode, out):
    cert = json.loads((PRIOR / "receipts.json").read_bytes())["files"]
    raw = (episode / "episode/run/report.json").read_bytes()
    decl = (episode / "predeclared.json").read_bytes()
    assert digest(raw) == cert["episode/run/report.json"]["sha256"]
    assert digest(decl) == cert["predeclared.json"]["sha256"]
    action, = [a for a in json.loads(raw)["execution"]["actions"] if a["result_step"] == 21]
    args = action["arguments"]
    assert action["tool"] == "search_files" and action["executed"] and args == {"pattern": "tui-replay", "dir": "src/main.rs"}
    source = (episode / "baseline" / args["dir"]).read_bytes()
    assert digest(source) == json.loads(decl)["baseline_inventory"][args["dir"]]["sha256"]
    observations = {"coder": [], "observer": []}
    roles = []

    class Handler(BaseHTTPRequestHandler):
        def log_message(self, *_):
            pass

        def do_POST(self):
            body = json.loads(self.rfile.read(int(self.headers["Content-Length"])))
            messages = body["messages"]
            role = "observer" if self.path.startswith("/observer/") else "coder"
            if role == "coder":
                for msg in messages:
                    if msg.get("role") == "tool" and msg.get("tool_call_id") == CALL:
                        observations[role].append(msg["content"])
                delta = {"content": "The CLI route is still unfinished."}
                if "coder" not in roles:
                    delta = {"content": (
                        "<plan>\ngoal: Implement the new CLI route in src/main.rs\nsteps:\n1) Locate the sibling command in src/main.rs\n"
                        "2) Wire the new command\n3) Verify the CLI behavior\nacceptance: The new command runs and is verified\n"
                        "risks: Editing an inventory entry may miss dispatch\nassumptions: A sibling exists\n</plan>\n"
                        "<think>\ngoal: Locate the sibling in src/main.rs\nstep: 1\ntool: search_files\nrisk: A text hit may be a comment\n"
                        "doubt: Where the sibling is defined\nnext: search_files tui-replay in src/main.rs\nverify: Read the matching source\n</think>"),
                        "tool_calls": [{"index": 0, "id": CALL, "function": {"name": "search_files", "arguments": json.dumps(args)}}]}
            else:
                packet = json.loads(messages[-1]["content"].split("<packet>\n")[-1].split("\n</packet>")[0])
                observations[role].extend(a["result"] for a in packet["execution_evidence"]["actions"]
                                          if a["call_id"] == CALL and a["executed"])
                delta = {"content": json.dumps({"summary": "End the local delivery fixture.", "suggestions": [
                    {"kind": "clarify", "confidence": 0.2, "reason": "Delivery captured.", "suggested_tool": None, "suggested_args": {}}]})}
            roles.append(role)
            finish = "tool_calls" if "tool_calls" in delta else "stop"
            payload = ("data: " + json.dumps({"choices": [{"delta": delta, "finish_reason": finish}]}) + "\n\ndata: [DONE]\n\n").encode()
            self.send_response(200)
            self.send_header("Content-Type", "text/event-stream")
            self.send_header("Content-Length", str(len(payload)))
            self.end_headers()
            self.wfile.write(payload)

    out.mkdir(exist_ok=False)
    with tempfile.TemporaryDirectory(dir=out) as tmp, ThreadingHTTPServer(("127.0.0.1", 0), Handler) as server:
        root = Path(tmp) / "source"
        (root / "src").mkdir(parents=True)
        (root / args["dir"]).write_bytes(source)
        prompt = out / "task.txt"
        prompt.write_text("Implement a new CLI command in src/main.rs, wire its dispatch, and verify the actual invocation. "
                          "First search src/main.rs for the sibling tui-replay. Report remaining work truthfully.\n")
        server_thread = threading.Thread(target=server.serve_forever, daemon=True)
        server_thread.start()
        address = f"http://127.0.0.1:{server.server_port}"
        try:
            cmd = [executor, "--provider", "openai", "--model", "fixture", "--base-url", address + "/coder", "--api-key", "fixture",
                   "coupled-run", "--tool-root", root, "--prompt-file", prompt, "--out-dir", out / "run", "--max-requests", "10",
                   "--coder-chunk-requests", "4", "--observer-reserved-requests", "2", "--observer-provider", "openai",
                   "--observer-model", "fixture", "--observer-base-url", address + "/observer", "--observer-api-key-env", "LOCAL_FIXTURE_KEY"]
            with (out / "console.log").open("w") as log:
                subprocess.run(cmd, cwd=root, env={"HOME": tmp, "PATH": os.defpath, "LOCAL_FIXTURE_KEY": "fixture"},
                               stdout=log, stderr=subprocess.STDOUT, check=True, timeout=45)
        finally:
            server.shutdown()
            server_thread.join()
    report = json.loads((out / "run/report.json").read_bytes())
    native, = [a for a in report["execution"]["actions"] if a["call_id"] == CALL]
    if native["arguments"] != args or not native["executed"]:
        raise ValueError("R7 search did not execute as authored")
    rows = {}
    for role, seen in observations.items():
        if not seen:
            raise ValueError(f"no native search delivered to {role}")
        text = seen[0]
        rows[role] = {"native_tool_output": text, "navigation": [s for s in text.splitlines() if s.startswith(MARKER)],
                      "literal_rows": [s for s in text.splitlines() if s.startswith(args["dir"] + ":")]}
        assert rows[role]["literal_rows"] == [s for s in action["result"].splitlines() if s.startswith(args["dir"] + ":")]
    return {"r7_step": 21, "r7_call_id": action["call_id"], "arguments": args, "source_sha256": digest(source),
            "executor_sha256": digest(executor.read_bytes()), "external_provider_requests": 0, "local_role_requests": roles,
            "report_sha256": digest((out / "run/report.json").read_bytes()), "role_delivery": rows}


if __name__ == "__main__":
    parser = argparse.ArgumentParser(description=__doc__)
    for name in ("executor", "r7_episode", "out"):
        parser.add_argument(name, type=Path)
    opt = parser.parse_args()
    print(json.dumps(replay(opt.executor.resolve(), opt.r7_episode.resolve(), opt.out.resolve()), indent=2))
