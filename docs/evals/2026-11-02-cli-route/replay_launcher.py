"""Post-run control for the single-file and mixed-file npm edits authored in R8."""
import argparse
import hashlib
import http.client
import json
import os
from pathlib import Path
import socket
import subprocess
import sys
import tempfile
import time

HERE = Path(__file__).resolve().parent
sys.path.insert(0, str(HERE.parents[2] / "scripts"))
from coupled_bench.common import inventory, read_json, sha
from self_dogfood.contracts import execution_env, git_snapshot
from self_dogfood.runner import cache_lock


def post(port, args):
    conn = http.client.HTTPConnection("127.0.0.1", port, timeout=10)
    try:
        conn.request("POST", "/api/apply_diff", json.dumps(args), {"Content-Type": "application/json"})
        response = conn.getresponse()
        return {"status": response.status, "body": json.load(response)}
    finally:
        conn.close()


def replay(repo, episode, executor, target):
    cert = read_json(HERE / "receipts.json")["files"]

    def pinned(name):
        if sha(episode / name) != cert[name]["sha256"]:
            raise ValueError(f"wrong sealed file {name}")
        return read_json(episode / name)

    plan = read_json(HERE / "plan.json")
    assert sha(executor) == plan["executor_sha256"]
    declaration = pinned("predeclared.json")
    report = pinned("episode/run/report.json")
    session = pinned("episode/run/session.json")
    calls = {c["id"]: c for m in session["messages"] for c in m.get("tool_calls", [])}
    edits = []
    for step in (21, 23):
        a, = [r for r in report["execution"]["actions"] if r["result_step"] == step]
        args = json.loads(calls[a["call_id"]]["function"]["arguments"])
        assert a["tool"] == "apply_diff" and args == a["arguments"] and args["path"] == "npm/spiral-coder/lib/launcher.cjs"
        edits.append(args)
    package_test = episode / "inputs/packaged_spiral.py"
    assert sha(package_test) == declaration["inputs_inventory"][package_test.name]["sha256"]
    patch = HERE / "candidate.patch"
    assert sha(patch) == cert["episode/candidate.patch"]["sha256"]
    with tempfile.TemporaryDirectory() as tmp, cache_lock(target):
        scratch = Path(tmp)
        root = scratch / "source"
        git_snapshot(repo, plan["baseline_commit"], root)
        subprocess.run(["git", "apply", "--binary", str(patch)], cwd=root, check=True)
        env = dict(execution_env(scratch, target), CARGO_NET_OFFLINE="true")
        for cmd in [["cargo", "clean", "--locked", "--offline", "-p", "spiral-coder"],
                    ["cargo", "build", "--locked", "--offline", "--bin", "spiral-coder"]]:
            subprocess.run(cmd, cwd=root, env=env, check=True, stdout=subprocess.DEVNULL)
        binary = target / "debug/spiral-coder"

        def package():
            res = subprocess.run([sys.executable, "-BS", package_test, binary], cwd=root, env=env, capture_output=True, timeout=90)
            return {"exit_code": res.returncode, "routed_to_tui": b"spiral routed replay listing to tui" in res.stderr}

        before = package()
        direct = subprocess.run([binary, "tui-replay-list", "--filter", "PREFS", "--max-cases", "1"], cwd=root,
                                env=env, capture_output=True, timeout=10, check=True)
        initial = inventory(root)
        with socket.socket() as sock:
            sock.bind(("127.0.0.1", 0))
            port = sock.getsockname()[1]
        server = subprocess.Popen([executor, "serve", "--host", "127.0.0.1", "--port", str(port)], cwd=root,
                                  stdin=subprocess.DEVNULL, stdout=subprocess.DEVNULL, stderr=subprocess.DEVNULL,
                                  env={"HOME": tmp, "PATH": os.defpath})
        try:
            for _ in range(100):
                if server.poll() is not None:
                    raise RuntimeError("local file-tool server exited")
                try:
                    with socket.create_connection(("127.0.0.1", port), timeout=0.2):
                        break
                except OSError:
                    time.sleep(0.025)
            else:
                raise RuntimeError("local file-tool server not ready")
            combined = post(port, edits[1])
            assert combined["status"] == 400 and inventory(root) == initial
            single = post(port, edits[0])
            now = inventory(root)
            changed = sorted(k for k in now.keys() | initial.keys() if now.get(k) != initial.get(k))
            assert single["status"] == 200 and changed == [edits[0]["path"]]
        finally:
            server.terminate()
            server.wait(timeout=5)
        after = package()
        assert before == {"exit_code": 1, "routed_to_tui": True} and after["exit_code"] == 0
        return {"experiment": "post-run native file-tool replay on a disposable copy",
                "run_receipt_sha256": cert["run-receipt.json"]["sha256"], "candidate_patch_sha256": sha(patch),
                "executor_sha256": sha(executor), "candidate_binary_sha256": sha(binary),
                "external_provider_requests": 0, "native_filtered_stdout": json.loads(direct.stdout),
                "package_before": before, "mixed_step": 23, "mixed_http_status": combined["status"],
                "mixed_changed_paths": [], "single_step": 21, "single_http_status": single["status"],
                "authored_diff_sha256": {str(n): hashlib.sha256(a["diff"].encode()).hexdigest()
                                          for n, a in zip((21, 23), edits)},
                "single_changed_paths": changed, "package_after_single": after}


if __name__ == "__main__":
    parser = argparse.ArgumentParser(description=__doc__)
    for name in ("repo", "episode", "executor", "target"):
        parser.add_argument(name, type=Path)
    args = parser.parse_args()
    print(json.dumps(replay(args.repo.resolve(), args.episode.resolve(), args.executor.resolve(), args.target.resolve()), indent=2))
