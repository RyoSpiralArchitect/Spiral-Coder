"""Replay H3's original done request and its line-break-only correction."""

from http.server import BaseHTTPRequestHandler, ThreadingHTTPServer
import argparse
import json
import os
from pathlib import Path
import shutil
import subprocess
import sys
import tempfile
import threading

HERE = Path(__file__).resolve().parent
ROOT = HERE.parents[2]
sys.path.insert(0, str(ROOT / "scripts"))
from coupled_bench.common import inventory, read_json, sha
from self_dogfood.evidence import bound_run


def replay(raw, binary, split_line):
    archive = HERE.parent / "2026-11-03-search-walk/h3"
    receipt = read_json(archive / "results.json")["run_receipt_sha256"]
    assert bound_run(
        raw, read_json(archive / "plan.json")["declaration_sha256"], receipt
    )
    recording = raw / "episode/run"
    source = read_json(recording / "session.json")
    report = read_json(recording / "report.json")
    done = report["execution"]["actions"][-1]
    assert done["tool"] == "done" and done["result"] == "OK: done"
    messages = source["messages"]
    index = next(
        i
        for i, m in enumerate(messages)
        if any(c["id"] == done["call_id"] for c in m.get("tool_calls", []))
    )
    assistant = messages[index]
    source["messages"] = messages[:index]
    args = done["arguments"].copy()
    original = args["summary"]
    assert (
        original.count(". Before: FAILED\n") == 1
        and "After: PASSED" in original.splitlines()
    )
    if split_line:
        args["summary"] = original.replace(". Before: FAILED\n", ".\nBefore: FAILED\n")
    proposed = False

    class Handler(BaseHTTPRequestHandler):
        def log_message(self, *_):
            pass

        def do_POST(self):
            nonlocal proposed
            self.rfile.read(int(self.headers["Content-Length"]))
            if proposed:
                self.send_response(400)
                self.end_headers()
                return
            proposed = True
            call = {
                "index": 0,
                "id": "h3-report-replay",
                "type": "function",
                "function": {"name": "done", "arguments": json.dumps(args)},
            }
            delta = {
                "choices": [
                    {
                        "delta": {
                            "content": assistant.get("content") or "",
                            "tool_calls": [call],
                        },
                        "finish_reason": "tool_calls",
                    }
                ]
            }
            wire = ("data: " + json.dumps(delta) + "\n\ndata: [DONE]\n\n").encode()
            self.send_response(200)
            self.send_header("Content-Type", "text/event-stream")
            self.send_header("Content-Length", str(len(wire)))
            self.end_headers()
            self.wfile.write(wire)

    with (
        tempfile.TemporaryDirectory() as tmp,
        ThreadingHTTPServer(("127.0.0.1", 0), Handler) as server,
    ):
        scratch = Path(tmp).resolve()
        root = scratch / "source"
        old_root = raw / "episode/workspace"
        shutil.copytree(old_root, root, ignore=shutil.ignore_patterns(".git"))
        before = inventory(root)
        (scratch / "seed.json").write_text(
            json.dumps(source).replace(str(old_root), str(root))
        )
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
                    "--tool-root",
                    root,
                    "--prompt-file",
                    raw / "episode/task.txt",
                    "--seed-session",
                    scratch / "seed.json",
                    "--without-observer",
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
        assert before == inventory(root) and proposed
        result = read_json(scratch / "run/report.json")
        (attempt,) = [
            a
            for a in result["execution"]["actions"]
            if a["call_id"] == "h3-report-replay"
        ]
    return {
        "executor_sha256": sha(binary),
        "source_run_receipt_sha256": receipt,
        "swapped_one_space_for_newline": split_line,
        "accepted": attempt["result"] == "OK: done",
        "native_response": attempt["result"],
        "workspace_unchanged": True,
        "external_requests": 0,
    }


if __name__ == "__main__":
    p = argparse.ArgumentParser(description=__doc__)
    p.add_argument("raw", type=Path)
    p.add_argument("binary", type=Path)
    args = p.parse_args()
    print(
        json.dumps(
            [
                replay(args.raw.resolve(), args.binary.resolve(), split)
                for split in (False, True)
            ],
            indent=2,
        )
    )
