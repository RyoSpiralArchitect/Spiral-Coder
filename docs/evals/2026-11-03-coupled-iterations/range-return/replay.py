"""Replay both rejected Observer replies against the saved native state on loopback."""
import argparse
from http.server import BaseHTTPRequestHandler, ThreadingHTTPServer
import json
import os
from pathlib import Path
import shutil
import subprocess
import tempfile
import threading
import hashlib

HERE=Path(__file__).resolve().parent


def run(binary, previous):
    parent=json.loads((previous/"receipt.json").read_bytes())
    expected=json.loads((HERE.parent/"results.json").read_bytes())["chain_receipt_sha256"]
    sha=lambda data:hashlib.sha256(data).hexdigest()
    assert sha((previous/"receipt.json").read_bytes())==expected
    messages=json.loads((HERE.parent/"observer-refusals.json").read_bytes())
    replies=[m.get("raw_content") or m["content"] for m in messages if m["role"]=="Assistant"]
    assert len(replies)==2
    ledger=[]
    class Handler(BaseHTTPRequestHandler):
        def log_message(self,*args):
            pass
        def do_POST(self):
            data=json.loads(self.rfile.read(int(self.headers["Content-Length"])))
            if "tools" in data:
                hints=[m["content"] for m in data["messages"] if m["content"].startswith("[Native read window]")]
                ledger.append({"role":"coder","native_window":hints})
                if hints:
                    self.send_error(400,"end of model-free wire capture")
                    return
                text="The work remains unfinished."
            else:
                n=sum(r["role"]=="observer" for r in ledger)
                ledger.append({"role":"observer"})
                if n>=len(replies):
                    self.send_error(400,"unexpected additional Observer request")
                    return
                text=replies[n]
            body=("data: "+json.dumps({"choices":[{"delta":{"content":text},"finish_reason":"stop"}]})+"\n\ndata: [DONE]\n\n").encode()
            self.send_response(200);self.send_header("Content-Type","text/event-stream")
            self.send_header("Content-Length",str(len(body)));self.end_headers();self.wfile.write(body)

    with tempfile.TemporaryDirectory() as td:
        tmp=Path(td);src=previous/"workspace";root=tmp/"workspace"
        shutil.copytree(src,root,ignore=shutil.ignore_patterns(".git"))
        prev=previous/"run/iteration-01/session.json"
        assert sha(prev.read_bytes())==parent["run_inventory"]["iteration-01/session.json"]["sha256"]
        seed=tmp/"seed.json";seed.write_text(prev.read_text().replace(str(src),str(root)))
        with ThreadingHTTPServer(("127.0.0.1",0),Handler) as server:
            th=threading.Thread(target=server.serve_forever,daemon=True);th.start()
            try:
                url=f"http://127.0.0.1:{server.server_port}/v1"
                process=subprocess.run([binary,"--provider","openai","--model","offline","--api-key","fixture",
                    "--base-url",url,"--timeout-seconds","3","coupled-run","--tool-root",root,"--prompt-file",previous/"task.txt",
                    "--seed-session",seed,"--out-dir",tmp/"capture","--max-requests","32","--coder-chunk-requests","8",
                    "--observer-reserved-requests","8","--transport-repair-requests","4"],cwd=root,env={"PATH":os.defpath,"HOME":td},
                    capture_output=True,timeout=35)
            finally:server.shutdown();th.join()
        report=json.loads((tmp/"capture/report.json").read_bytes())
        trace=[json.loads(l) for l in (tmp/"capture/trace.jsonl").read_bytes().splitlines()]
        windows=[h for r in ledger for h in r.get("native_window",[])]
        events=[e["data"] for e in trace if e["event"]=="observer_return_for_delivery"]
        assert sum(r["role"]=="observer" for r in ledger)==2
        return dict(source_receipt=expected,binary_sha256=sha(binary.read_bytes()),request_roles=[r["role"] for r in ledger],
            authored_reply_digests=[sha(v.encode()) for v in replies],native_windows_on_coder_wire=windows,
            native_scheduler_events=events,termination=report["termination"],exit=process.returncode,
            assistant_actions_executed_in_probe=bool(report["iteration_progress"]["changed_files"]),external_provider_requests=0)


if __name__=="__main__":
    p=argparse.ArgumentParser(description=__doc__)
    p.add_argument("binary",type=Path);p.add_argument("previous",type=Path)
    args=p.parse_args()
    print(json.dumps(run(args.binary.resolve(),args.previous.resolve()),indent=2))
