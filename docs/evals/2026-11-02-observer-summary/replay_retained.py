"""Replay the two sealed R6 Observer replies against the saved native session, without models."""
import argparse
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
PRIOR = HERE.parent / "2026-11-02-cargo-inputs"


def digest(data):
    return hashlib.sha256(data).hexdigest()


def load_r6(root):
    manifest = json.loads((PRIOR / "receipts.json").read_bytes())["files"]

    def pinned(name):
        data = (root / name).read_bytes()
        if digest(data) != manifest[name]["sha256"]:
            raise ValueError(f"wrong R6 artifact: {name}")
        return data

    report = json.loads(pinned("episode/run/report.json"))
    session = json.loads(pinned("episode/run/session.json"))
    config, = [json.loads(line)["data"] for line in pinned("episode/run/trace.jsonl").splitlines()
               if json.loads(line)["event"] == "verification_config"]
    wanted = [r["raw_sha256"] for r in json.loads((PRIOR / "cargo-observations.json").read_bytes())["post_run_duplicate_diagnosis"]]
    replies = [m.get("raw_content") or m["content"] for m in report["observer_display"] if m["role"] == "Assistant"]
    conflict, repeated = [next(raw for raw in replies if digest(raw.encode()) == sha) for sha in wanted]
    # Independent single-occurrence control for these specific pinned bytes.
    prefix, delimiter, tail = repeated.rstrip().rpartition(',"summary":')
    control = prefix + "}"
    if not delimiter or not tail.endswith("}") or json.loads(tail[:-1]) != json.loads(control)["summary"]:
        raise ValueError("unexpected structure in pinned repeated summary")
    return session["messages"], config["test_command"], [conflict, repeated, control]


def evaluate(binary, root):
    messages, test_command, raws = load_r6(root)
    with socket.socket() as sock:
        sock.bind(("127.0.0.1", 0))
        port = sock.getsockname()[1]
    rows = []
    with tempfile.TemporaryDirectory() as temp:
        proc = subprocess.Popen([str(binary), "serve", "--host", "127.0.0.1", "--port", str(port)],
                                cwd=temp, env={"HOME": temp, "PATH": os.defpath}, stdin=subprocess.DEVNULL,
                                stdout=subprocess.DEVNULL, stderr=subprocess.DEVNULL)
        try:
            for _ in range(100):
                if proc.poll() is not None:
                    raise RuntimeError("local server exited before replay")
                try:
                    with socket.create_connection(("127.0.0.1", port), timeout=0.2):
                        break
                except OSError:
                    time.sleep(0.025)
            else:
                raise RuntimeError("local server not ready")
            for label, raw in zip(["conflict", "identical_summary", "single_summary_control"], raws):
                payload = {"packet": {}, "messages": messages, "test_command": test_command,
                           "reason": "verification_failure", "response": raw}
                conn = http.client.HTTPConnection("127.0.0.1", port, timeout=10)
                try:
                    conn.request("POST", "/api/observer_action", json.dumps(payload), {"Content-Type": "application/json"})
                    res = conn.getresponse()
                    if res.status != 200:
                        raise RuntimeError(f"local evaluator: HTTP {res.status}")
                    data = json.load(res)
                finally:
                    conn.close()
                obs = data["packet"]["task_obligations"]
                proof = data["packet"]["execution_evidence"]
                action = (data["envelope"] or {"suggestions": [None]})["suggestions"][0]
                rows.append({"case": label, "raw_sha256": digest(raw.encode()), "issue": data["issue"],
                             "format_recovery": data["format_recovery"], "hint": data["hint"] is not None,
                             "contract": data["response_contract"] is not None,
                             "action": {k: action[k] for k in ("kind", "suggested_tool", "suggested_args", "confidence")} if action else None,
                             "pending_exact_checks": obs["pending_exact_checks"],
                             "available_checks": proof["available_checks"],
                             "required_diagnostic_read": obs["required_diagnostic_read"],
                             "pending_recovery": obs["pending_recovery"],
                             "advice_sha256": digest(json.dumps([data[k] for k in ("envelope", "hint", "response_contract")], sort_keys=True).encode())})
        finally:
            proc.terminate()
            proc.wait(timeout=5)
    return {"executor_sha256": digest(binary.read_bytes()), "provider_requests": 0,
            "native_actions_executed_by_replay": 0, "test_command": test_command, "results": rows}


if __name__ == "__main__":
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("executor", type=Path)
    parser.add_argument("r6_episode_directory", type=Path)
    args = parser.parse_args()
    print(json.dumps(evaluate(args.executor.resolve(), args.r6_episode_directory), indent=2))
