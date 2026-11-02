"""Replay either the R9 patch or its preceding symbol search through the normal native Coder."""

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
sys.path.insert(0, str(HERE.parents[2] / "scripts"))
from coupled_bench.common import inventory, read_json, sha
from self_dogfood.contracts import execution_env, git_snapshot
from self_dogfood.runner import cache_lock

CALL = "scope-replay-patch"


def run(repo, episode, executor, target, out, *, lookup=False):
    cert = read_json(HERE / "receipts.json")["files"]

    def load(name):
        if sha(episode / name) != cert[name]["sha256"]:
            raise ValueError(f"wrong sealed file {name}")
        return read_json(episode / name)

    session = load("episode/run/session.json")
    actions = load("episode/run/report.json")["execution"]["actions"]
    if lookup:
        action = next(a for a in actions if a["result_step"] == 13)
        assert action["tool"] == "search_files" and action["arguments"] == {
            "dir": "src",
            "pattern": "TuiReplay",
        }
        assert not action["executed"] and "[Progress Gate]" in action["result"]
    else:
        action = next(a for a in actions if a.get("edit_outcome") == "changed")
        assert (
            action["tool"] == "patch_file"
            and action["arguments"]["path"] == "src/task_scope.rs"
        )
    args = action["arguments"]
    (original,) = [
        c
        for m in session["messages"]
        for c in m.get("tool_calls", [])
        if c["id"] == action["call_id"]
    ]
    assert json.loads(original["function"]["arguments"]) == args
    previous = next(a for a in actions if a["result_step"] == action["result_step"] - 1)
    (end,) = [
        i + 1
        for i, m in enumerate(session["messages"])
        if m.get("tool_call_id") == previous["call_id"]
    ]
    session["messages"] = session["messages"][:end]
    declaration = load("predeclared.json")
    task = episode / "episode/task.txt"
    assert sha(task) == declaration["prompt_sha256"]
    sent = False

    class Handler(BaseHTTPRequestHandler):
        def log_message(self, *_):
            pass

        def do_POST(self):
            nonlocal sent
            self.rfile.read(int(self.headers["Content-Length"]))
            if sent:
                # Stop the fixture after the one proposed edit.
                self.send_response(400)
                self.end_headers()
                self.wfile.write(b"fixture ended after first action")
                return
            sent = True
            text = (
                "<evidence>\ntarget_files: 1) src/task_scope.rs\ntarget_symbols: 1) module documentation\n"
                "evidence: The preceding native read shows the module-level scope comment.\nopen_questions: none\n"
                "next_probe: patch_file src/task_scope.rs\n</evidence>\n"
                "<think>\ngoal: Update the observed source documentation\nstep: 1\ntool: patch_file\nrisk: wrong scope\n"
                "doubt: The route is still unfinished\nnext: patch_file src/task_scope.rs\nverify: Run the exact visible test\n</think>"
            )
            if lookup:
                text = (
                    "<think>\ngoal: Locate the sibling command and its callers\nstep: 1\ntool: search_files\n"
                    "risk: missing dispatch\ndoubt: its location is unknown\nnext: search_files for TuiReplay in src\n"
                    "verify: Inspect the returned source locations\n</think>"
                )
            call = {"index": 0, "id": CALL, "function": original["function"]}
            delta = {
                "choices": [
                    {
                        "delta": {"content": text, "tool_calls": [call]},
                        "finish_reason": "tool_calls",
                    }
                ]
            }
            payload = ("data: " + json.dumps(delta) + "\n\ndata: [DONE]\n\n").encode()
            self.send_response(200)
            self.send_header("Content-Type", "text/event-stream")
            self.send_header("Content-Length", str(len(payload)))
            self.end_headers()
            self.wfile.write(payload)

    out.mkdir(exist_ok=False)
    seed = out / "seed.json"
    seed.write_text(json.dumps(session))
    with (
        tempfile.TemporaryDirectory(dir=out) as tmp,
        cache_lock(target),
        ThreadingHTTPServer(("127.0.0.1", 0), Handler) as server,
    ):
        scratch = Path(tmp)
        root = scratch / "source"
        git_snapshot(repo, declaration["baseline_commit"], root)
        fixture = episode / "inputs/catalog_visible.py"
        assert sha(fixture) == declaration["inputs_inventory"][fixture.name]["sha256"]
        shutil.copyfile(fixture, root / "scripts/catalog_visible.py")
        before = inventory(root) if lookup else sha(root / args["path"])
        env = dict(execution_env(scratch, target), CARGO_NET_OFFLINE="true")
        subprocess.run(
            ["cargo", "clean", "--locked", "--offline", "-p", "spiral-coder"],
            cwd=root,
            env=env,
            check=True,
            capture_output=True,
        )
        thread = threading.Thread(target=server.serve_forever, daemon=True)
        thread.start()
        try:
            cmd = [
                executor,
                "--provider",
                "openai",
                "--model",
                "fixture",
                "--base-url",
                f"http://127.0.0.1:{server.server_port}",
                "--api-key",
                "fixture",
                "coupled-run",
                "--tool-root",
                root,
                "--prompt-file",
                task,
                "--seed-session",
                seed,
                "--out-dir",
                out / "run",
                "--max-requests",
                "8",
                "--without-observer",
            ]
            with (out / "console.log").open("w") as log:
                subprocess.run(
                    cmd,
                    cwd=root,
                    env=env,
                    stdout=log,
                    stderr=subprocess.STDOUT,
                    timeout=240,
                )
        finally:
            server.shutdown()
            thread.join()
        after = inventory(root) if lookup else sha(root / args["path"])
    report = read_json(out / "run/report.json")
    (attempt,) = [a for a in report["execution"]["actions"] if a["call_id"] == CALL]
    assert attempt["arguments"] == args
    trace = list(map(json.loads, (out / "run/trace.jsonl").read_bytes().splitlines()))
    targets = [
        r["data"].get("next_target")
        for r in trace
        if r["event"] == "meta_harness_policy"
        and r["data"].get("action") == "mutate_existing_now"
    ]
    output = attempt.get("result") or ""
    if lookup:
        return {
            "executor_sha256": sha(executor),
            "run_receipt_sha256": cert["run-receipt.json"]["sha256"],
            "original_call_id": action["call_id"],
            "original_step": action["result_step"],
            "arguments": args,
            "executed": attempt["executed"],
            "same_tool": attempt["tool"] == action["tool"],
            "result_header": output.splitlines()[0],
            "source_locations": [
                m.group(0).rstrip(":")
                for line in output.splitlines()
                if (m := re.match(r"^[^:\s]+:[1-9][0-9]*:", line))
            ][:6],
            "main_hits": [
                line[:200]
                for line in output.splitlines()
                if line.startswith("src/main.rs:")
            ][:6],
            "changed_paths": sorted(
                k for k in before.keys() | after.keys() if before.get(k) != after.get(k)
            ),
            "external_provider_requests": 0,
        }
    return {
        "executor_sha256": sha(executor),
        "run_receipt_sha256": cert["run-receipt.json"]["sha256"],
        "original_call_id": action["call_id"],
        "original_step": action["result_step"],
        "path": args["path"],
        "file_before_sha256": before,
        "file_after_sha256": after,
        "changed": before != after,
        "executed": attempt["executed"],
        "same_arguments": True,
        "result_header": output.splitlines()[0],
        "scope_feedback": next(
            (s for s in output.splitlines() if s.startswith("[Edit Scope]")), None
        ),
        "forced_edit_targets": targets,
        "local_requests": report["role_requests"],
        "external_provider_requests": 0,
    }


if __name__ == "__main__":
    parser = argparse.ArgumentParser(description=__doc__)
    for n in ("repo", "episode", "executor", "target", "out"):
        parser.add_argument(n, type=Path)
    parser.add_argument(
        "--lookup",
        action="store_true",
        help="submit the original step 13 search instead of the step 14 patch",
    )
    args = parser.parse_args()
    print(
        json.dumps(
            run(
                *(
                    getattr(args, n).resolve()
                    for n in ("repo", "episode", "executor", "target", "out")
                ),
                lookup=args.lookup,
            ),
            indent=2,
        )
    )
