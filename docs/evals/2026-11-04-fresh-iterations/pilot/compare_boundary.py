"""Replay the stopped live pilot on loopback and capture the first request after it."""
import argparse
from http.server import BaseHTTPRequestHandler, ThreadingHTTPServer
import json
from pathlib import Path
import re
import shutil
import subprocess
import sys
import tempfile
import threading

HERE = Path(__file__).resolve().parent
REPO = HERE.parents[3]
sys.path.insert(0, str(REPO / "scripts"))
from coupled_bench.common import clean_env, inventory, read_json, sha


def compare(source, binary):
    plan, run = read_json(HERE / "plan.json"), read_json(HERE / "results.json")
    assert sha(source / "receipt.json") == run["receipt_sha256"]
    assert inventory(source / "run") == read_json(source / "receipt.json")["run_inventory"]
    child = source / "run/iteration-01"
    report = read_json(child / "report.json")
    messages = read_json(child / "session.json")["messages"]
    coder = []
    for m in messages:
        if not m.get("tool_calls"):
            continue
        text = m["content"]
        if text.startswith("[assistant-summary]"):
            pane = next(v["content"] for v in report["coder_display"] if v["role"] == "Assistant")
            text = re.search(r"(<plan>.*?</think>)", pane, re.S).group(1)
        calls = m["tool_calls"]
        for i, call in enumerate(calls):
            call["index"] = i
            args = json.loads(call["function"]["arguments"])
            if "check_id" in args:
                call["function"]["arguments"] = json.dumps({"check_id": args["check_id"]})
        coder.append(dict(content=text, tool_calls=calls))
    observer = [dict(content=m.get("raw_content") or m["content"]) for m in report["observer_display"] if m["role"] == "Assistant"]
    queues = dict(coder=coder, observer=observer)
    script = [(req["role"], queues[req["role"]].pop(0)) for req in report["work"]["requests"]]
    assert not any(queues.values()) and len(script) == run["manifest"]["wire_requests"]
    roles = []
    with tempfile.TemporaryDirectory(prefix="spiral-iteration-red-") as tmp:
        temp = Path(tmp)
        root = temp / "workspace"
        shutil.copytree(REPO / plan["fixture"] / "workspace", root)

        class Endpoint(BaseHTTPRequestHandler):
            def log_message(self, *args):
                pass

            def do_POST(self):
                req = json.loads(self.rfile.read(int(self.headers["Content-Length"])))
                role = "coder" if req.get("tools") else "observer"
                i = len(roles)
                roles.append(role)
                if i >= len(script) or script[i][0] != role:
                    self.send_error(400, "local control ends at the next wire")
                    return
                payload = ("data: " + json.dumps({"choices": [{"delta": script[i][1], "finish_reason": "stop"}]})
                           + "\n\ndata: [DONE]\n\n").replace(str(source / "workspace"), str(root)).encode()
                self.send_response(200)
                self.send_header("Content-Type", "text/event-stream")
                self.send_header("Content-Length", str(len(payload)))
                self.end_headers()
                self.wfile.write(payload)

        with ThreadingHTTPServer(("127.0.0.1", 0), Endpoint) as server:
            th = threading.Thread(target=server.serve_forever, daemon=True)
            th.start()
            argv = read_json(source / "started.json")["argv"]
            argv[0] = str(binary)
            url = f"http://127.0.0.1:{server.server_port}/v1"
            for flag, value in (("--base-url", url), ("--observer-base-url", url),
                                ("--tool-root", root), ("--out-dir", temp / "run"),
                                ("--observer-api-key-env", "PILOT_LOCAL_KEY")):
                argv[argv.index(flag) + 1] = str(value)
            argv[1:1] = ["--api-key", "fixture"]
            env = clean_env(binary)
            env.update(PILOT_LOCAL_KEY="fixture", PYTHONDONTWRITEBYTECODE="1")
            try:
                proc = subprocess.run(argv, cwd=root, env=env, timeout=45, capture_output=True)
            finally:
                server.shutdown()
                th.join()
        m = read_json(temp / "run/iterations.json")
        assert roles[:len(script)] == [role for role, _ in script] and inventory(root) == plan["baseline_inventory"]
        return dict(source_seal=run["receipt_sha256"], binary_sha256=sha(binary), recorded_replies=len(script),
                    actual_roles=roles, opened_second_iteration=(temp / "run/iteration-02").is_dir(),
                    first_progress=m["iterations"][0]["progress"], termination=m["stop_reason"],
                    exit_code=proc.returncode, unchanged_stock_workspace=True, external_requests=0)


if __name__ == "__main__":
    p = argparse.ArgumentParser(description=__doc__)
    p.add_argument("source", type=Path)
    p.add_argument("binary", type=Path)
    a = p.parse_args()
    print(json.dumps(compare(a.source.resolve(), a.binary.resolve()), indent=2))
