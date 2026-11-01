"""Check the two rejected r3 hunks against their actual pre-edit source using the frozen Rust file tool."""
import argparse
import hashlib
import json
import os
from pathlib import Path
import socket
import subprocess
import tempfile
import time
from urllib.error import HTTPError, URLError
from urllib.request import build_opener, ProxyHandler, Request

HERE = Path(__file__).resolve().parent
PATH = "src/tui_replay.rs"
PARTIAL = "call_tjAfUs94OVBaf6M58rBXC0mj"
REJECTED = ("call_JWcBJWIqfiXNrFPoWt1iBmlG", "call_N8mIZIjbwk4J14pfBVSbZ13o")


def digest(body):
    return hashlib.sha256(body).hexdigest()


def main(raw):
    plan = json.loads((HERE / "plan.json").read_text())
    receipts = json.loads((HERE / "receipts.json").read_text())["files"]
    report = (raw / "episode/run/report.json").read_bytes()
    trace = (raw / "episode/run/trace.jsonl").read_bytes()
    assert digest(report) == receipts["episode/run/report.json"]["sha256"]
    assert digest(trace) == receipts["episode/run/trace.jsonl"]["sha256"]
    declared = (raw / "predeclared.json").read_bytes()
    assert digest(declared) == plan["declaration_sha256"]
    original = (raw / "baseline" / PATH).read_bytes()
    assert digest(original) == json.loads(declared)["baseline_inventory"][PATH]["sha256"]
    binary = raw / "runtime/spiral-coder"
    assert digest(binary.read_bytes()) == plan["executor_sha256"]
    native = json.loads(report)["execution"]["actions"]
    end = next(a["result_step"] for a in native if a["call_id"] == REJECTED[-1])
    preceding = [a["call_id"] for a in native if a["executed"] and a["result_step"] <= end
                 and a["tool"] in ("apply_diff", "write_file", "patch_file") and a["arguments"].get("path") == PATH]
    assert preceding == [PARTIAL, *REJECTED], "reconstruct all prior edits to the probed file"
    calls = {r["data"]["id"]: json.loads(r["data"]["arguments"]) for l in trace.splitlines()
             if (r := json.loads(l))["event"] == "tool_call" and r["data"]["id"] in preceding}
    opener = build_opener(ProxyHandler({}))
    with tempfile.TemporaryDirectory() as tmp:
        root = Path(tmp).resolve()
        temp_file = root / PATH
        temp_file.parent.mkdir()
        temp_file.write_bytes(original)
        with socket.socket() as sock:
            sock.bind(("127.0.0.1", 0))
            port = sock.getsockname()[1]
        url = f"http://127.0.0.1:{port}"
        server = subprocess.Popen([binary, "serve", "--host", "127.0.0.1", "--port", str(port)],
                                  cwd=root, env={"PATH": os.defpath}, stdout=subprocess.DEVNULL, stderr=subprocess.DEVNULL)
        try:
            for _ in range(50):
                if server.poll() is not None:
                    raise RuntimeError("local server exited")
                try:
                    with opener.open(url + "/api/status", timeout=2) as response:
                        assert Path(json.load(response)["workspace_root"]).resolve() == root
                    break
                except URLError:
                    time.sleep(0.1)
            else:
                raise RuntimeError("local server did not become ready")

            def apply(args):
                req = Request(url + "/api/apply_diff", data=json.dumps(args).encode(),
                              headers={"Content-Type": "application/json"})
                try:
                    with opener.open(req, timeout=5) as response:
                        return response.status, json.load(response)["message"].splitlines()[0]
                except HTTPError as error:
                    with error:
                        return error.code, json.load(error)["error"].splitlines()[0]

            code, message = apply(calls[PARTIAL])
            assert code == 200 and message.startswith("OK: applied 3/5")
            source = temp_file.read_bytes()
            assert source != original
            results = []
            for call_id in REJECTED:
                args = calls[call_id]
                diff = args["diff"]
                assert diff.endswith("\n*** End Patch")
                unchanged_status, rejection = apply(args)
                assert unchanged_status == 400 and temp_file.read_bytes() == source
                # Only remove the bare protocol marker. Keep every hunk and context byte.
                changed_status, accepted = apply(dict(args, diff=diff.removesuffix("*** End Patch")))
                changed = temp_file.read_bytes()
                results.append({"call_id": call_id, "original_diff_sha256": digest(diff.encode()),
                                "original_status": unchanged_status, "original_result": rejection,
                                "without_trailer_status": changed_status, "without_trailer_result": accepted,
                                "file_changed": changed != source, "trimmed_result_sha256": digest(changed),
                                "resolve_path_definitions_after": changed.count(b"fn resolve_path(")})
                temp_file.write_bytes(source)
        finally:
            server.terminate()
            server.wait(timeout=10)
    return {"type":"post-run file tool diagnosis", "runtime_commit":plan["runtime_commit"],
            "pre_edit_source_sha256":digest(source), "prior_edit":message,
            "provider_calls":0, "results":results}


if __name__ == "__main__":
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("episode", type=Path)
    print(json.dumps(main(parser.parse_args().episode.resolve()),indent=2))
