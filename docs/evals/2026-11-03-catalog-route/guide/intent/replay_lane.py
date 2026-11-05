"""Capture native task classification before any tool can run, using only a loopback provider."""

import argparse
from http.server import BaseHTTPRequestHandler, ThreadingHTTPServer
import json
import os
from pathlib import Path
import shutil
import subprocess
import sys
import tempfile
import threading

HERE = Path(__file__).resolve().parent
REPO = HERE.parents[4]
sys.path.insert(0, str(REPO / "scripts"))
from coupled_bench.common import inventory, read_json, sha
from self_dogfood.evidence import bound_run


def capture(binary, raw, inspect):
    pin = read_json(HERE.parent / "plan.json")["declaration_sha256"]
    receipt = read_json(HERE.parent / "results.json")["run_receipt_sha256"]
    assert bound_run(raw, pin, receipt)
    prompt = (raw / "episode/task.txt").read_text()
    if inspect:
        prompt = "Read only: inspect the catalog documentation. Do not edit anything."
    payloads = []

    class Handler(BaseHTTPRequestHandler):
        def log_message(self, *_):
            pass

        def do_POST(self):
            value = json.loads(self.rfile.read(int(self.headers["Content-Length"])))
            payloads.append([t["function"]["name"] for t in value.get("tools", [])])
            self.send_response(400)
            self.end_headers()
            self.wfile.write(b"lane captured")

    with (
        tempfile.TemporaryDirectory() as tmp,
        ThreadingHTTPServer(("127.0.0.1", 0), Handler) as server,
    ):
        scratch = Path(tmp).resolve()
        root = scratch / "source"
        shutil.copytree(raw / "baseline", root)
        before = inventory(root)
        task = scratch / "task.txt"
        task.write_text(prompt)
        thread = threading.Thread(target=server.serve_forever, daemon=True)
        thread.start()
        try:
            subprocess.run(
                [
                    binary,
                    "--provider",
                    "openai",
                    "--model",
                    "fixture",
                    "--api-key",
                    "fixture",
                    "--base-url",
                    f"http://127.0.0.1:{server.server_port}",
                    "coupled-run",
                    "--without-observer",
                    "--tool-root",
                    root,
                    "--prompt-file",
                    task,
                    "--out-dir",
                    scratch / "run",
                    "--max-requests",
                    "8",
                ],
                cwd=root,
                env={"PATH": os.defpath, "HOME": str(scratch)},
                capture_output=True,
                timeout=45,
            )
        finally:
            server.shutdown()
            thread.join()
        trace = [
            json.loads(line)
            for line in (scratch / "run/trace.jsonl").read_bytes().splitlines()
        ]
        lanes = [e["data"] for e in trace if e["event"] == "task_harness"]
        assert len(lanes) == 1 and len(payloads) == 1 and inventory(root) == before
        assert not any(e["event"] == "tool_call" for e in trace)
    return {
        "executor_sha256": sha(binary),
        "d1_receipt": receipt,
        "input": "explicit_inspection" if inspect else "original_d1",
        "native_task_harness": lanes[0],
        "external_requests": 0,
        "file_actions": 0,
    }


if __name__ == "__main__":
    p = argparse.ArgumentParser(description=__doc__)
    p.add_argument("binary", type=Path)
    p.add_argument("episode", type=Path)
    a = p.parse_args()
    print(
        json.dumps(
            [
                capture(a.binary.resolve(), a.episode.resolve(), i)
                for i in (False, True)
            ],
            indent=2,
        )
    )
