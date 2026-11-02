"""Compare native shared advice on three unmodified prefixes of the sealed R8 session."""

import argparse
from contextlib import contextmanager
import hashlib
import http.client
import json
import os
from pathlib import Path
import socket
import subprocess
import tempfile
import time

HERE = Path(__file__).resolve().parent
PRIOR = HERE.parent / "2026-11-02-cli-route"


def digest(data):
    return hashlib.sha256(data).hexdigest()


def prefixes(episode):
    manifest = json.loads((PRIOR / "receipts.json").read_bytes())["files"]

    def load(name):
        data = (episode / name).read_bytes()
        if digest(data) != manifest[name]["sha256"]:
            raise ValueError(f"wrong R8 {name}")
        return json.loads(data)

    report = load("episode/run/report.json")
    session = load("episode/run/session.json")
    by_step = {a["result_step"]: a for a in report["execution"]["actions"]}
    messages = session["messages"]

    def stop(step):
        (match,) = [
            i
            for i, m in enumerate(messages)
            if m.get("tool_call_id") == by_step[step]["call_id"]
        ]
        return match + 1

    return session, by_step, {step: stop(step) for step in (21, 22, 23)}


@contextmanager
def evaluator(binary):
    with tempfile.TemporaryDirectory() as tmp:
        with socket.socket() as sock:
            sock.bind(("127.0.0.1", 0))
            port = sock.getsockname()[1]
        server = subprocess.Popen(
            [binary, "serve", "--host", "127.0.0.1", "--port", str(port)],
            cwd=tmp,
            env={"PATH": os.defpath, "HOME": tmp},
            stdin=subprocess.DEVNULL,
            stdout=subprocess.DEVNULL,
            stderr=subprocess.DEVNULL,
        )
        try:
            for _ in range(100):
                if server.poll() is not None:
                    raise RuntimeError("local server exited")
                try:
                    with socket.create_connection(("127.0.0.1", port), timeout=0.2):
                        break
                except OSError:
                    time.sleep(0.025)
            else:
                raise RuntimeError("local server not ready")

            def proposal(messages, command):
                conn = http.client.HTTPConnection("127.0.0.1", port, timeout=20)
                try:
                    body = {"packet": {}, "test_command": command, "messages": messages}
                    conn.request(
                        "POST",
                        "/api/observer_action",
                        json.dumps(body),
                        {"Content-Type": "application/json"},
                    )
                    res = conn.getresponse()
                    if res.status != 200:
                        raise ValueError(f"native evaluator returned {res.status}")
                    return json.load(res)["packet"]["task_obligations"].get(
                        "deferred_edit"
                    )
                finally:
                    conn.close()

            yield proposal
        finally:
            server.terminate()
            server.wait(timeout=5)


def replay(binary, episode):
    session, actions, ends = prefixes(episode)
    rows = []
    with evaluator(binary) as evaluate:
        for step, end in ends.items():
            p = evaluate(session["messages"][:end], "cargo test 2>&1")
            if p is not None and (
                p["suggested_args"] != actions[21]["arguments"]
                or p["proposal_call_id"] != actions[21]["call_id"]
                or p["verification_call_id"] != actions[22]["call_id"]
            ):
                raise ValueError("proposal changed or attributed to the wrong call")
            rows.append(
                {
                    "through_step": step,
                    "available": p is not None,
                    "path": p["suggested_args"]["path"] if p else None,
                    "authored_diff_sha256": digest(p["suggested_args"]["diff"].encode())
                    if p
                    else None,
                    "proposal_call_id": p["proposal_call_id"] if p else None,
                    "verification_call_id": p["verification_call_id"] if p else None,
                }
            )
    return {
        "executor_sha256": digest(binary.read_bytes()),
        "external_provider_requests": 0,
        "edits_executed": 0,
        "prefixes": rows,
    }


if __name__ == "__main__":
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("executor", type=Path)
    parser.add_argument("episode", type=Path)
    args = parser.parse_args()
    print(json.dumps(replay(args.executor.resolve(), args.episode.resolve()), indent=2))
