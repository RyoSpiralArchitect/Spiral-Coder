"""Replay both rounds locally. The optional predeclared tail permits one live report."""
import argparse
import copy
import hashlib
from http.server import BaseHTTPRequestHandler, ThreadingHTTPServer
import json
import os
from pathlib import Path
import re
import shutil
import sys
import threading
from urllib.request import Request, urlopen

HERE = Path(__file__).resolve().parent
REPO = HERE.parents[3]
sys.path.insert(0, str(REPO / "scripts"))
from coupled_bench.common import clean_env, inventory, read_json, sha, write_json
from self_dogfood.process import run_process


def recorded_replies(original, root):
    replies = []
    seen = set()
    for part in ("iteration-01", "iteration-02"):
        r = original / "run" / part
        report = read_json(r / "report.json")
        coder = []
        for message in read_json(r / "session.json")["messages"]:
            calls = copy.deepcopy(message.get("tool_calls", []))
            if not calls or calls[0]["id"] in seen:
                continue
            seen.add(calls[0]["id"])
            text = message["content"]
            if text.startswith("[assistant-summary]"):
                pane = next(m["content"] for m in report["coder_display"] if m["role"] == "Assistant")
                text = re.search(r"(<plan>.*?</think>)", pane, re.S).group(1)
            for i, call in enumerate(calls):
                call["index"] = i
                func = call["function"]
                args = json.loads(func["arguments"].replace(str(original / "workspace"), str(root)))
                if "check_id" in args:
                    # Native receipts expand the model's selected ID to the bound command and cwd.
                    args = {"check_id": args["check_id"]}
                func["arguments"] = json.dumps(args)
            coder.append({"content": text.replace(str(original / "workspace"), str(root)), "tool_calls": calls})
        observer = [{"content": m.get("raw_content") or m["content"]} for m in report["observer_display"]
                    if m["role"] == "Assistant"]
        queues = {"coder": coder, "observer": observer}
        for entry in report["work"]["requests"]:
            role = entry["role"]
            replies.append((role, queues[role].pop(0)))
        assert not any(queues.values())
    return replies


def run(binary, original, dest, live):
    case = read_json(HERE / "plan.json")
    stored = read_json(original / "receipt.json")
    assert sha(original / "receipt.json") == read_json(HERE / "results.json")["sealed_receipt_sha256"]
    assert inventory(original / "run") == stored["run_inventory"]
    assert inventory(original / "workspace") == stored["final_inventory"]
    key = os.environ["OPENAI_API_KEY"].strip() if live else ""
    if live:
        pin = read_json(HERE / "tail-plan.json")
        assert sha(binary) == pin["fixed_executor_sha256"] and sha(__file__) == pin["replay_driver_sha256"]
        assert sha(original / "receipt.json") == pin["original_receipt_sha256"] and key
        controls = read_json(HERE / "wire-controls.json")
        assert sha(HERE / "wire-controls.json") == pin["controls_sha256"]
        assert controls["old"]["report_only_boundary"] == [] and controls["fixed"]["report_only_boundary"]
        assert all(v["prefix_matched"] and v["matched_original_final_workspace"]
                   and not v["errors"] and v["live_upstream_calls"] == 0 for v in controls.values())
    dest.mkdir()
    root = dest / "workspace"
    shutil.copytree(REPO / case["fixture"] / "workspace", root)
    assert inventory(root) == case["baseline_inventory"]
    replies = recorded_replies(original, root)
    seen = []
    sent = []
    captured = []
    errors = []

    class Handler(BaseHTTPRequestHandler):
        def log_message(self, *args):
            pass

        def do_POST(self):
            body = self.rfile.read(int(self.headers["Content-Length"]))
            data = json.loads(body)
            tools = [t["function"]["name"] for t in data.get("tools", [])]
            role = "coder" if tools else "observer"
            index = len(seen)
            seen.append(role)
            if index < len(replies) and role == replies[index][0]:
                delta = replies[index][1]
                payload = ("data: " + json.dumps({"choices": [{"delta": delta, "finish_reason": "stop"}]})
                           + "\n\ndata: [DONE]\n\n").encode()
            elif (index == len(replies) and role == "coder" and tools == ["done"]
                  and inventory(root) == stored["final_inventory"]
                  and data.get("model") == case["model"] and b"one report-only request" in body):
                captured.append({"tools": tools, "sha256": hashlib.sha256(body).hexdigest()})
                if not live:
                    self.send_error(400, "local control stops at the report wire")
                    return
                sent.append(role)
                req = Request("https://api.openai.com/v1/chat/completions", data=body,
                              headers={"Authorization": "Bearer " + key, "Content-Type": "application/json"})
                try:
                    with urlopen(req, timeout=90) as remote:
                        payload = remote.read(4 * 1024 * 1024)
                    assert key.encode() not in payload
                    (dest / "live-report.sse").write_bytes(payload)
                except Exception as exc:
                    errors.append(type(exc).__name__)
                    self.send_error(400, "single report call failed")
                    return
            else:
                errors.append("unexpected prefix or report-only boundary")
                self.send_error(400, "unexpected request")
                return
            self.send_response(200)
            self.send_header("Content-Type", "text/event-stream")
            self.send_header("Content-Length", str(len(payload)))
            self.end_headers()
            self.wfile.write(payload)

    with ThreadingHTTPServer(("127.0.0.1", 0), Handler) as server:
        worker = threading.Thread(target=server.serve_forever, daemon=True)
        worker.start()
        url = f"http://127.0.0.1:{server.server_port}/v1"
        argv = read_json(original / "started.json")["argv"]
        argv[0] = str(binary)
        for opt, value in (("--base-url", url), ("--observer-base-url", url), ("--tool-root", root),
                           ("--out-dir", dest / "run"), ("--observer-api-key-env", "PILOT_FIXTURE_KEY")):
            argv[argv.index(opt) + 1] = str(value)
        argv[1:1] = ["--api-key", "fixture"]
        env = clean_env(binary)
        env.update(PILOT_FIXTURE_KEY="fixture", PYTHONDONTWRITEBYTECODE="1", PYTHONPYCACHEPREFIX=str(dest / "cache"))
        try:
            process = run_process(argv, root, env, 150, dest / "console.log")
        finally:
            server.shutdown()
            worker.join()
    manifest = read_json(dest / "run/iterations.json")
    reports = [read_json(dest / "run" / row["directory"] / "report.json") for row in manifest["iterations"]]
    final = reports[-1]
    done = [dict(step=x["result_step"], outcome=x["result"], args=x["arguments"]) for x in final["execution"]["actions"]
            if x["tool"] == "done" and x["executed"] and x["result"] == "OK: done"]
    art = {"original_receipt_sha256": sha(original / "receipt.json"), "binary_sha256": sha(binary),
           "process": process, "replayed_prefix_count": min(len(seen), len(replies)), "request_roles": seen,
           "prefix_matched": seen[:len(replies)] == [role for role, _ in replies],
           "matched_original_final_workspace": inventory(root) == stored["final_inventory"],
           "manifest": manifest, "per_round_roles": [r["role_requests"] for r in reports],
           "report_only_boundary": captured, "live_upstream_calls": len(sent), "errors": errors,
           "accepted_done": done, "pending_checks": final["execution"]["pending_exact_checks"],
           "observer_assists_in_last_round": len(final["work"]["assists"]), "run_inventory": inventory(dest / "run")}
    write_json(dest / "result.json", art)
    return art


if __name__ == "__main__":
    pa = argparse.ArgumentParser(description=__doc__)
    for name in ("binary", "original", "destination"):
        pa.add_argument(name, type=Path)
    pa.add_argument("--live", action="store_true")
    opt = pa.parse_args()
    r = run(opt.binary.resolve(), opt.original.resolve(), opt.destination.resolve(), opt.live)
    print(json.dumps({k: r[k] for k in ("prefix_matched", "matched_original_final_workspace",
                                        "report_only_boundary", "live_upstream_calls", "errors")}))
