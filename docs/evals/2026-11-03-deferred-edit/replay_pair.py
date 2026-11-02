"""Follow the post-check R8 proposal through the native paired loop with two loopback responders."""

import argparse
from http.server import BaseHTTPRequestHandler, ThreadingHTTPServer
import json
from pathlib import Path
import shutil
import subprocess
import sys
import tempfile
import threading

HERE = Path(__file__).resolve().parent
sys.path.insert(0, str(HERE))
from replay_advice import prefixes, PRIOR, digest

sys.path.insert(0, str(HERE.parents[2] / "scripts"))
from coupled_bench.common import inventory, read_json, sha
from self_dogfood.contracts import execution_env, git_snapshot
from self_dogfood.runner import cache_lock

TAG = "Retry the retained one-file proposal."
CALL = "r8-resubmitted-launcher"


def replay(repo, episode, binary, target, out):
    session, actions, ends = prefixes(episode)
    assert not any(
        a.get("edit_outcome") == "changed"
        for a in actions.values()
        if a["result_step"] > 22
    )
    session["messages"] = session["messages"][: ends[22]]
    original = actions[21]["arguments"]
    original_call = next(
        m
        for m in session["messages"]
        if any(c["id"] == actions[21]["call_id"] for c in m.get("tool_calls", []))
    )
    expected_sha = digest(original["diff"].encode())
    out.mkdir(exist_ok=False)
    seed = out / "seed.json"
    seed.write_text(json.dumps(session))
    received = {"coder": [], "observer": []}
    roles = []
    sent = False

    class Handler(BaseHTTPRequestHandler):
        def log_message(self, *_):
            pass

        def do_POST(self):
            nonlocal sent
            body = json.loads(self.rfile.read(int(self.headers["Content-Length"])))
            role = "observer" if self.path.startswith("/observer/") else "coder"
            roles.append(role)
            messages = body["messages"]
            if role == "observer":
                packet = json.loads(
                    messages[-1]["content"]
                    .split("<packet>\n")[-1]
                    .split("\n</packet>")[0]
                )
                p = packet["task_obligations"].get("deferred_edit")
                if p is not None and not sent:
                    suggestion = {
                        "kind": "edit",
                        "confidence": 0.95,
                        "reason": TAG,
                        "suggested_tool": p["suggested_tool"],
                        "suggested_args": p["suggested_args"],
                        "based_on": ["task_obligations"],
                    }
                else:
                    suggestion = {
                        "kind": "clarify",
                        "confidence": 0.2,
                        "reason": "End the local fixture.",
                        "suggested_tool": None,
                        "suggested_args": {},
                    }
                delta = {
                    "content": json.dumps(
                        {
                            "summary": suggestion["reason"],
                            "suggestions": [suggestion],
                            "response_contract": {
                                "required": True,
                                "focus_axes": ["correctness"],
                                "note": "Review the observed launcher.",
                            },
                        }
                    )
                }
            else:
                views = []
                for m in messages:
                    s = m.get("content") or ""
                    if m["role"] == "system" and s.startswith(
                        "[Shared task obligations]"
                    ):
                        views.append(json.loads(s.splitlines()[-1]))
                p = views[-1].get("deferred_edit") if views else None
                advisory = any(
                    m["role"] == "system"
                    and "<observer_advisory " in str(m.get("content"))
                    and TAG in str(m.get("content"))
                    for m in messages
                )
                delta = {
                    "content": "The package and documentation are still incomplete."
                }
                if p is not None and advisory and not sent:
                    sent = True
                    prelude = (
                        "[Observer response]\ndecision: accept\nreason: Retry the single original launcher edit after its visible check passed.\n"
                        "<evidence>\ntarget_files: 1) npm/spiral-coder/lib/launcher.cjs\ntarget_symbols: 1) commands\n"
                        "evidence: The prior read of npm/spiral-coder/lib/launcher.cjs shows the commands set containing tui-replay.\n"
                        "open_questions: none\nnext_probe: apply_diff to the launcher commands set\n</evidence>\n"
                    )
                    delta = {
                        "content": prelude + original_call["content"],
                        "tool_calls": [
                            {
                                "index": 0,
                                "id": CALL,
                                "function": {
                                    "name": p["suggested_tool"],
                                    "arguments": json.dumps(p["suggested_args"]),
                                },
                            }
                        ],
                    }
            if p is not None:
                assert (
                    p["suggested_args"] == original
                    and p["proposal_call_id"] == actions[21]["call_id"]
                )
                received[role].append(digest(p["suggested_args"]["diff"].encode()))
            reason = "tool_calls" if "tool_calls" in delta else "stop"
            payload = (
                "data: "
                + json.dumps({"choices": [{"delta": delta, "finish_reason": reason}]})
                + "\n\ndata: [DONE]\n\n"
            ).encode()
            self.send_response(200)
            self.send_header("Content-Type", "text/event-stream")
            self.send_header("Content-Length", str(len(payload)))
            self.end_headers()
            self.wfile.write(payload)

    manifest = read_json(PRIOR / "receipts.json")["files"]
    assert sha(episode / "predeclared.json") == manifest["predeclared.json"]["sha256"]
    declaration = read_json(episode / "predeclared.json")
    patch = PRIOR / "candidate.patch"
    assert sha(patch) == manifest["episode/candidate.patch"]["sha256"]
    prompt = episode / "episode/task.txt"
    assert sha(prompt) == declaration["prompt_sha256"]
    with (
        tempfile.TemporaryDirectory(dir=out) as tmp,
        cache_lock(target),
        ThreadingHTTPServer(("127.0.0.1", 0), Handler) as server,
    ):
        scratch = Path(tmp)
        root = scratch / "source"
        git_snapshot(repo, declaration["baseline_commit"], root)
        subprocess.run(["git", "apply", "--binary", str(patch)], cwd=root, check=True)
        for name in ("catalog_visible.py", "packaged_spiral.py"):
            assert (
                sha(episode / "inputs" / name)
                == declaration["inputs_inventory"][name]["sha256"]
            )
        shutil.copyfile(
            episode / "inputs/catalog_visible.py", root / "scripts/catalog_visible.py"
        )
        before = inventory(root)
        env = dict(
            execution_env(scratch, target),
            CARGO_NET_OFFLINE="true",
            LOCAL_FIXTURE_KEY="fixture",
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
                "--seed-session",
                seed,
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
                "LOCAL_FIXTURE_KEY",
            ]
            with (out / "console.log").open("w") as log:
                subprocess.run(
                    cmd,
                    cwd=root,
                    env=env,
                    stdout=log,
                    stderr=subprocess.STDOUT,
                    check=True,
                    timeout=240,
                )
        finally:
            server.shutdown()
            thread.join()
        after = inventory(root)
        changed = sorted(
            n for n in before.keys() | after.keys() if before.get(n) != after.get(n)
        )
        report = read_json(out / "run/report.json")
        retries = [a for a in report["execution"]["actions"] if a["call_id"] == CALL]
        assists = [
            r
            for r in report["work"]["assists"]
            if CALL
            in (r.get("suggestion_execution") or {}).get("matching_call_ids", [])
        ]
        subprocess.run(
            ["cargo", "build", "--locked", "--offline", "--bin", "spiral-coder"],
            cwd=root,
            env=env,
            capture_output=True,
            check=True,
        )
        package = subprocess.run(
            [
                sys.executable,
                "-BS",
                episode / "inputs/packaged_spiral.py",
                target / "debug/spiral-coder",
            ],
            cwd=root,
            env=env,
            capture_output=True,
            timeout=90,
        )
        return {
            "executor_sha256": sha(binary),
            "original_step": 21,
            "seed_ends_at_step": 22,
            "authored_diff_sha256": expected_sha,
            "external_provider_requests": 0,
            "local_roles": roles,
            "offer_digests_received": received,
            "changed_paths": changed,
            "authored_retry": sent,
            "retry_results": [
                {
                    "executed": a["executed"],
                    "outcome": a["edit_outcome"],
                    "same_arguments": a["arguments"] == original,
                    "result_header": (a.get("result") or "").splitlines()[0],
                }
                for a in retries
            ],
            "matching_assists": [
                {"id": r["assist_id"], "decision": r["decision"]} for r in assists
            ],
            "packaged_oracle_exit": package.returncode,
            "report_sha256": sha(out / "run/report.json"),
        }


if __name__ == "__main__":
    parser = argparse.ArgumentParser(description=__doc__)
    for name in ("repo", "episode", "executor", "target", "out"):
        parser.add_argument(name, type=Path)
    args = parser.parse_args()
    print(
        json.dumps(
            replay(
                *(
                    getattr(args, n).resolve()
                    for n in ("repo", "episode", "executor", "target", "out")
                )
            ),
            indent=2,
        )
    )
