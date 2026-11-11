"""Model-free request capture plus native Observer projection parity.

The loopback endpoint returns a deliberate 400 error, never a tool call. No
provider key is loaded and no model/tool execution is used for these controls.
Raw requests remain in the caller's private output directory.
"""
import argparse
from contextlib import contextmanager
import hashlib
from http.server import BaseHTTPRequestHandler, ThreadingHTTPServer
import json
from pathlib import Path
import subprocess
import sys
import threading


def sha(path):
    return hashlib.sha256(Path(path).read_bytes()).hexdigest()


def write(path, value):
    path.write_text(json.dumps(value, ensure_ascii=False, indent=2) + "\n")


def size(value):
    return len(json.dumps(value, ensure_ascii=False, separators=(",", ":")).encode())


@contextmanager
def capture_server():
    requests = []

    class Handler(BaseHTTPRequestHandler):
        def log_message(self, *args):
            pass

        def do_POST(self):
            requests.append(json.loads(self.rfile.read(int(self.headers["Content-Length"]))))
            body = json.dumps({"error": {"message": "Controlled capture: no model response"}}).encode()
            self.send_response(400)
            self.send_header("Content-Type", "application/json")
            self.send_header("Content-Length", str(len(body)))
            self.end_headers()
            self.wfile.write(body)

    server = ThreadingHTTPServer(("127.0.0.1", 0), Handler)
    thread = threading.Thread(target=server.serve_forever, daemon=True)
    thread.start()
    try:
        yield f"http://127.0.0.1:{server.server_port}/v1", requests
    finally:
        server.shutdown()
        server.server_close()
        thread.join()


def main():
    parser = argparse.ArgumentParser()
    for key in ["repo", "old", "new", "seed", "task", "out"]:
        parser.add_argument("--" + key, type=Path, required=True)
    parser.add_argument("--observer-session", type=Path, action="append", default=[])
    args = parser.parse_args()
    args.out.mkdir(parents=True, exist_ok=False)
    sys.path.insert(0, str(args.repo / "scripts"))
    from self_dogfood.contracts import execution_env
    from coupled_bench.common import inventory
    sys.path.insert(0, str(args.repo / "docs/evals/2026-11-03-deferred-edit"))
    from replay_advice import native_evaluator

    seed = json.loads(args.seed.read_bytes())
    workspace = Path(seed["tool_root"])
    before = inventory(workspace, include_nested_git=True)
    summary = {"external_provider_calls": 0, "tool_execution": False,
               "seed_sha256": sha(args.seed), "task_sha256": sha(args.task),
               "binaries": {"old": sha(args.old), "new": sha(args.new)},
               "coder": {}, "observer": []}
    for label, binary in [("old", args.old), ("new", args.new)]:
        out = args.out / label
        out.mkdir()
        with capture_server() as (url, requests):
            command = [str(binary), "--provider", "openai", "--model", "gpt-5.6-luna",
                       "--base-url", url, "--temperature", "0", "--max-tokens", "32768",
                       "--timeout-seconds", "10", "coupled-run", "--tool-root", str(workspace),
                       "--prompt-file", str(args.task), "--out-dir", str(out / "run"),
                       "--seed-session", str(args.seed), "--max-requests", "2",
                       "--observer-reserved-requests", "1", "--transport-repair-requests", "0",
                       "--observer-provider", "openai", "--observer-model", "gpt-5.6-luna",
                       "--observer-base-url", url, "--observer-api-key-env", "OPENAI_API_KEY",
                       "--observer-max-tokens", "8192"]
            env = execution_env(out, out / "build", binary)
            env["OPENAI_API_KEY"] = "local-capture-only"
            with (out / "console.log").open("wb") as log:
                process = subprocess.run(command, cwd=workspace, env=env, stdin=subprocess.DEVNULL,
                                         stdout=log, stderr=subprocess.STDOUT, timeout=45)
            write(out / "requests.json", requests)
            if process.returncode and "Controlled capture: no model response" not in (out / "console.log").read_text():
                raise RuntimeError(f"{label} unexpected capture failure: inspect console.log")
        coder = [req for req in requests if req.get("tools")]
        assert len(coder) == 1, (label, len(coder))
        payload = coder[0]
        summary["coder"][label] = {"process_exit_code": process.returncode, "payload_bytes": size(payload), "messages_bytes": size(payload["messages"]),
                                   "message_count": len(payload["messages"]),
                                   "tools_sha256": hashlib.sha256(json.dumps(payload["tools"], sort_keys=True).encode()).hexdigest(),
                                   "capture_sha256": sha(out / "requests.json")}
        report = json.loads((out / "run/report.json").read_bytes())
        prior_ids = {call["id"] for msg in seed["messages"] for call in msg.get("tool_calls", [])}
        assert not [a for a in report["execution"]["actions"] if a["call_id"] not in prior_ids]
        assert inventory(workspace, include_nested_git=True) == before
    assert summary["coder"]["old"]["tools_sha256"] == summary["coder"]["new"]["tools_sha256"]

    with native_evaluator(args.old) as old, native_evaluator(args.new) as new:
        for path in [args.seed, *args.observer_session]:
            messages = json.loads(path.read_bytes())["messages"]
            request = {"packet": {"work_budget": {"remaining": 16, "coder_remaining": 12}},
                       "messages": messages, "lang": "en", "reason": "incomplete_task"}
            left, right = old(request), new(request)
            for key in ["packet", "native_messages", "completion_issue", "issue"]:
                assert left[key] == right[key], (path.name, key)
            summary["observer"].append({"session_sha256": sha(path), "messages": len(messages),
                                         "old_prompt_bytes": len(left["prompt"].encode()),
                                         "new_prompt_bytes": len(right["prompt"].encode()),
                                         "native_packet_equal": True})
            index = len(summary["observer"])
            write(args.out / f"observer-{index}.json", {"old": left, "new": right})
    summary["candidate_unchanged"] = inventory(workspace, include_nested_git=True) == before
    write(args.out / "results.json", summary)
    print(json.dumps(summary, indent=2))


if __name__ == "__main__":
    main()
