"""Record two genuine first-iteration interruptions from a supplied native writer."""
import argparse
from http.server import BaseHTTPRequestHandler, HTTPServer
import json
import os
from pathlib import Path
import subprocess
from threading import Thread


def record(writer, directory, spent):
    directory.mkdir()
    workspace = directory / "workspace"
    workspace.mkdir()
    prompt = directory / "task.txt"
    prompt.write_text("Read the local workspace and report.\n")
    run = directory / "run"
    calls = []

    class LocalReply(BaseHTTPRequestHandler):
        def do_POST(self):
            self.rfile.read(int(self.headers["Content-Length"]))
            calls.append(1)
            if spent:
                # Force a local report-write error after the request has been recorded.
                (run / "iteration-01/report.json").mkdir(exist_ok=True)
            body = b'{"error":{"message":"local fixture response"}}'
            self.send_response(400)
            self.send_header("Content-Type", "application/json")
            self.send_header("Content-Length", str(len(body)))
            self.end_headers()
            self.wfile.write(body)

        def log_message(self, *args):
            pass

    with HTTPServer(("127.0.0.1", 0), LocalReply) as server:
        cmd = [writer, "--provider", "openai", "--model", "offline", "--api-key", "offline",
               "--base-url", f"http://127.0.0.1:{server.server_port}/v1", "--timeout-seconds", "2",
               "coupled-run", "--iterations", "2", "--max-requests", "4", "--tool-root", workspace,
               "--prompt-file", prompt, "--out-dir", run]
        if not spent:
            seed = directory / "incomplete-session.json"
            seed.write_text("{\n")
            cmd.extend(["--seed-session", seed])
        thread = Thread(target=server.serve_forever, daemon=True)
        thread.start()
        try:
            proc = subprocess.run(cmd, env={"HOME": str(directory), "PATH": os.defpath},
                                  capture_output=True, text=True, timeout=30)
        finally:
            server.shutdown()
            thread.join()
    (directory / "writer.log").write_text(proc.stdout + proc.stderr)
    manifest = json.loads((run / "iterations.json").read_bytes())
    expected = 1 if spent else None
    assert proc.returncode and len(calls) == int(spent)
    assert manifest["iterations"] == [] and manifest["stop_reason"] == "interrupted"
    assert manifest["interrupted_iteration"] == dict(directory="iteration-01", task_requests=expected,
                                                      wire_requests=expected)
    assert (manifest["task_requests"], manifest["wire_requests"]) == (expected, expected)
    ledger = run / "iteration-01/state/tui-work.json"
    if spent:
        requests = json.loads(ledger.read_bytes())["requests"]
        assert [(r["charge"], r["status"]) for r in requests] == [("task", "http_400")]
    else:
        assert not ledger.exists()
    return {"loopback_requests": len(calls), "external_requests": 0, "writer_exit": proc.returncode,
            "manifest": manifest}


if __name__ == "__main__":
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("writer", type=Path)
    parser.add_argument("output", type=Path)
    args = parser.parse_args()
    output = args.output.resolve()
    output.mkdir()
    result = {name: record(args.writer.resolve(), output / name, spent)
              for name, spent in (("missing", False), ("spent", True))}
    print(json.dumps(result, indent=2))
