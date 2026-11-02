"""Replay the r4 leaf write in the native loop with two local HTTP responders, without provider credentials."""

import argparse
import hashlib
from http.server import BaseHTTPRequestHandler, ThreadingHTTPServer
import json
from pathlib import Path
import shutil
import subprocess
import sys
import tempfile
import threading

HERE = Path(__file__).resolve().parent
sys.path.insert(0, str(HERE.parents[2] / "scripts"))
from coupled_bench.common import clean_env, inventory, read_json, sha
from self_dogfood.evidence import bound_run

R4_PIN = "cd7f77d44bf4970d5e58d569d350e0981fc930c926bbe3269b36ef7c8637fe73"
R4_RUN = "a47299b5a707a6a316771cb97ba212f4821dd2f8fad681927dd6acde9b646252"
LEAF = "src/tui_replay/catalog.rs"
CALL = "call_p7O6Zy68wKdqlv77zu6ifP0Z"


def replay(raw, binary, out, seed, executor_sha):
    assert sha(raw / "predeclared.json") == R4_PIN and bound_run(raw, R4_PIN, R4_RUN)
    assert sha(binary) == executor_sha
    by_call = [json.loads(line) for line in (raw / "episode/run/trace.jsonl").read_bytes().splitlines()]
    args = next(
        json.loads(r["data"]["arguments"])
        for r in by_call
        if r["event"] == "tool_call" and r["data"]["id"] == CALL
    )
    assert args["path"] == LEAF
    out.mkdir(parents=True, exist_ok=False)
    requests = []
    evidence = {"coder": [], "observer": []}

    class Handler(BaseHTTPRequestHandler):
        def log_message(self, *_):
            pass

        def do_POST(self):
            body = json.loads(self.rfile.read(int(self.headers["Content-Length"])))
            role = "observer" if self.path.startswith("/observer/") else "coder"
            if role == "coder":
                for msg in body["messages"]:
                    marker = "[Cargo compiler inputs for this target, not runtime coverage] "
                    if msg["role"] == "tool" and marker in msg["content"]:
                        evidence[role].append(json.loads(msg["content"].rsplit(marker, 1)[1]))
                text = "The helper still needs integration."
                delta = {"content": text}
                if "coder" not in requests:
                    prelude = (
                        "<plan>\ngoal: Add the module\nsteps:\n1) Create "
                        + LEAF
                        + "\n2) Verify the result\nacceptance: module created and tested\nrisks: unregistered module\nassumptions: path is known\n</plan>\n"
                        "<think>\ngoal: Create module\nstep: 1\ntool: write_file\nrisk: missing registration\ndoubt: compile reach\nnext: write_file "
                        + LEAF
                        + "\nverify: cargo test\n</think>"
                    )
                    delta = {
                        "content": prelude,
                        "tool_calls": [
                            {
                                "index": 0,
                                "id": "leaf-replay",
                                "function": {"name": "write_file", "arguments": json.dumps(args)},
                            }
                        ],
                    }
            else:
                packet = json.loads(
                    body["messages"][-1]["content"].split("<packet>\n")[-1].split("\n</packet>")[0]
                )
                evidence[role].extend(
                    a["cargo_inputs"]
                    for a in packet["execution_evidence"]["actions"]
                    if "cargo_inputs" in a
                )
                delta = {
                    "content": json.dumps(
                        {
                            "summary": "Inspect the module registration.",
                            "suggestions": [
                                {
                                    "kind": "read",
                                    "reason": "Locate the module boundary.",
                                    "confidence": 0.95,
                                    "suggested_tool": "read_file",
                                    "suggested_args": {"path": "src/tui_replay.rs"},
                                }
                            ],
                        }
                    )
                }
            requests.append(role)
            payload = (
                "data: "
                + json.dumps(
                    {
                        "choices": [
                            {
                                "delta": delta,
                                "finish_reason": "tool_calls" if "tool_calls" in delta else "stop",
                            }
                        ]
                    }
                )
                + "\n\ndata: [DONE]\n\n"
            ).encode()
            self.send_response(200)
            self.send_header("Content-Type", "text/event-stream")
            self.send_header("Content-Length", str(len(payload)))
            self.end_headers()
            self.wfile.write(payload)

    with (
        tempfile.TemporaryDirectory(dir=out) as temp,
        ThreadingHTTPServer(("127.0.0.1", 0), Handler) as server,
    ):
        root = Path(temp) / "source"
        target = Path(temp) / "target"
        if seed is not None:
            subprocess.run(["cp", "-cR", seed, target], check=True)
        shutil.copytree(raw / "baseline", root)
        assert inventory(root) == read_json(raw / "predeclared.json")["baseline_inventory"]
        prompt = out / "prompt.txt"
        contract = {
            "version": 1,
            "requirements": [
                {
                    "id": "integrate",
                    "description": "Create and register the catalog",
                    "artifacts": [
                        {"path": LEAF, "kind": "source"},
                        {"path": "src/tui_replay.rs", "kind": "source"},
                    ],
                    "checks": [{"command": "cargo test --locked --bin spiral-coder -q"}],
                }
            ],
        }
        prompt.write_text(
            "Create src/tui_replay/catalog.rs with the supplied Rust helper, register it in src/tui_replay.rs, and verify both edits. "
            "The final answer must name the changed paths and the observed verification outcome.\n"
            "<task_requirements>\n" + json.dumps(contract) + "\n</task_requirements>\n"
        )
        env = dict(
            clean_env(),
            CARGO_TARGET_DIR=str(target),
            CARGO_NET_OFFLINE="true",
            CARGO_INCREMENTAL="0",
            CARGO_PROFILE_TEST_DEBUG="0",
            CARGO_PROFILE_DEV_DEBUG="0",
            LOCAL_OBSERVER_KEY="fixture",
        )
        subprocess.run(
            ["cargo", "clean", "--locked", "--offline", "-p", "spiral-coder"],
            cwd=root,
            env=env,
            check=True,
            capture_output=True,
        )
        address = f"http://127.0.0.1:{server.server_port}"
        thread = threading.Thread(target=server.serve_forever, daemon=True)
        thread.start()
        try:
            cmd = [
                binary,
                "--provider",
                "openai",
                "--model",
                "fixture",
                "--base-url",
                address + "/coder",
                "--api-key",
                "fixture",
                "coupled-run",
                "--tool-root",
                root,
                "--prompt-file",
                prompt,
                "--out-dir",
                out / "run",
                "--max-requests",
                "10",
                "--coder-chunk-requests",
                "4",
                "--observer-reserved-requests",
                "2",
                "--observer-provider",
                "openai",
                "--observer-model",
                "fixture",
                "--observer-base-url",
                address + "/observer",
                "--observer-api-key-env",
                "LOCAL_OBSERVER_KEY",
            ]
            with (out / "console.log").open("w") as console:
                result = subprocess.run(
                    cmd, cwd=root, env=env, stdout=console, stderr=subprocess.STDOUT, timeout=300
                )
            assert (root / LEAF).read_bytes() == args["content"].encode(), "real r4 bytes must be written"
        finally:
            server.shutdown()
            thread.join()
    expected = {"target": "unit tests src/main.rs", "files": {LEAF: "not_listed"}}
    assert all(expected in evidence[role] for role in ("coder", "observer")), evidence
    return {
        "type": "offline replay of r4 leaf bytes on original baseline with scripted local roles",
        "external_provider_requests": 0,
        "local_requests": requests,
        "process_exit": result.returncode,
        "original_run_sha256": R4_RUN,
        "original_call": CALL,
        "leaf_sha256": hashlib.sha256(args["content"].encode()).hexdigest(),
        "executor_sha256": sha(binary),
        "coder_wire": expected,
        "observer_wire": expected,
        "report_sha256": sha(out / "run/report.json"),
        "session_sha256": sha(out / "run/session.json"),
    }


if __name__ == "__main__":
    p = argparse.ArgumentParser(description=__doc__)
    for name in ("r4", "binary", "out"):
        p.add_argument(name, type=Path)
    p.add_argument("--executor-sha256", help="Pinned comparison executor, defaults to the frozen r6 binary")
    p.add_argument(
        "--seed-cache",
        type=Path,
        help="Optional existing Cargo cache to clone into the disposable target on macOS",
    )
    a = p.parse_args()
    print(
        json.dumps(
            replay(
                a.r4.resolve(),
                a.binary.resolve(),
                a.out.resolve(),
                a.seed_cache.resolve() if a.seed_cache else None,
                a.executor_sha256 or read_json(HERE / "plan.json")["executor_sha256"],
            ),
            indent=2,
        )
    )
