"""Inspect the real outgoing Coder request on a disposable, loopback-only resume."""
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
sys.path.insert(0, str(HERE.parents[3] / "scripts"))
from coupled_bench.common import inventory, read_json, sha
from self_dogfood.evidence import bound_run

COMMAND = "python3 -S -m unittest -v scripts.catalog_visible"


def capture(binary, episode, isolated):
    plan = read_json(HERE.parent / "plan.json")
    receipt = read_json(HERE.parent / "results.json")["run_receipt_sha256"]
    assert bound_run(episode, plan["declaration_sha256"], receipt)
    report = read_json(episode / "episode/run/report.json")
    session = read_json(episode / "episode/run/session.json")
    actions = {a["result_step"]: a for a in report["execution"]["actions"]}
    edit = actions[17]
    assert (edit["tool"], edit["arguments"]["path"], edit["edit_outcome"]) == (
        "patch_file", "src/main.rs", "changed")
    assert not any(a.get("edit_outcome") == "changed" for i, a in actions.items() if i > 17)
    assert actions[1]["arguments"]["command"] == COMMAND and actions[2]["tool"] == "read_file"
    end = next(i + 1 for i, m in enumerate(session["messages"]) if m.get("tool_call_id") == edit["call_id"])
    prefix = session["messages"][:end]
    if isolated:
        # Keep actual correlated failure, read and repair records. This is not
        # the full cold resume and does not establish the live interaction.
        ids = {actions[i]["call_id"] for i in (1, 2, 17)}
        prefix = [m for m in prefix if m["role"] not in ("tool", "assistant", "user")
                  or m["role"] == "user" and m["content"] != "Continue."
                  or m.get("tool_call_id") in ids
                  or any(c["id"] in ids for c in m.get("tool_calls", []))]
    session["messages"] = prefix
    menus = []

    class Handler(BaseHTTPRequestHandler):
        def log_message(self, *_):
            pass

        def do_POST(self):
            body = json.loads(self.rfile.read(int(self.headers["Content-Length"])))
            props = next(t["function"]["parameters"]["properties"] for t in body["tools"]
                         if t["function"]["name"] == "exec")
            messages = [m.get("content") for m in body["messages"] if isinstance(m.get("content"), str)]
            contract = next(json.loads(s.split("[Next action contract]\n", 1)[1].splitlines()[0])
                            for s in reversed(messages) if "[Next action contract]\n" in s)
            menus.append({"phase": contract["phase"], "contract_recheck": contract.get("required_recheck"),
                          "command_enum": props["command"].get("enum"), "id_enum": props["check_id"].get("enum")})
            self.send_response(400)
            self.end_headers()
            self.wfile.write(b"fixture complete")

    with tempfile.TemporaryDirectory() as tmp, ThreadingHTTPServer(("127.0.0.1", 0), Handler) as server:
        scratch = Path(tmp).resolve()
        root = scratch / "source"
        original = episode / "episode/workspace"
        shutil.copytree(original, root, ignore=shutil.ignore_patterns(".git"))
        before = inventory(root)
        seed = scratch / "seed.json"
        seed.write_text(json.dumps(session).replace(str(original), str(root)))
        worker = threading.Thread(target=server.serve_forever, daemon=True)
        worker.start()
        try:
            subprocess.run([binary, "--provider", "openai", "--model", "fixture", "--base-url",
                            f"http://127.0.0.1:{server.server_port}", "--api-key", "fixture", "coupled-run",
                            "--without-observer", "--tool-root", root, "--prompt-file", episode / "episode/task.txt",
                            "--seed-session", seed, "--out-dir", scratch / "run", "--max-requests", "8"],
                           env={"PATH": os.defpath, "HOME": str(scratch)}, cwd=root, capture_output=True, timeout=40)
        finally:
            server.shutdown()
            worker.join()
        trace = [json.loads(l) for l in (scratch / "run/trace.jsonl").read_bytes().splitlines()]
        assert before == inventory(root) and len(menus) == 1, menus
        stage = next(x["data"]["recovery_stage"] for x in trace if x["event"] == "agent_iter")
    return {"executor_sha256": sha(binary), "source_run_receipt_sha256": receipt,
            "input": "isolated retained tool steps 1,2,17" if isolated else "full retained prefix through step 17",
            "recovery_stage": stage, "menu": menus[0], "external_requests": 0, "file_actions_executed": 0}


if __name__ == "__main__":
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("binary", type=Path)
    parser.add_argument("episode", type=Path)
    args = parser.parse_args()
    print(json.dumps([capture(args.binary.resolve(), args.episode.resolve(), isolated)
                      for isolated in (False, True)], indent=2))
