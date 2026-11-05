"""Replay the previously blocked formatting request against the actual saved conversation."""
from http.server import BaseHTTPRequestHandler, ThreadingHTTPServer
import json
from pathlib import Path
import shutil
import subprocess
import sys
import tempfile
import threading

HERE = Path(__file__).resolve().parent
sys.path.insert(0, str(HERE.parents[3]/"scripts"))
from coupled_bench.common import inventory, read_json, sha
from self_dogfood.contracts import execution_env


def probe(binary, prepared, target):
    seed = read_json(prepared / "seed.json")
    messages = seed["messages"]
    calls = {c["id"]: (m, c) for m in messages for c in m.get("tool_calls", [])}
    original = next((calls[m["tool_call_id"]] for m in reversed(messages)
                     if m.get("role") == "tool" and "[Recovery Gate] stage=diagnose" in m.get("content", "")
                     and m["tool_call_id"] in calls and
                     json.loads(calls[m["tool_call_id"]][1]["function"]["arguments"]).get("command") == "cargo fmt --all"))
    message, call = original
    wires = []
    with tempfile.TemporaryDirectory(prefix="spiral-format-replay-") as temp:
        area = Path(temp).resolve(); root = area / "workspace"
        shutil.copytree(prepared / "workspace", root, ignore=shutil.ignore_patterns(".git"))
        assert inventory(root) == read_json(prepared / "initial-inventory.json")
        clone = area / "seed.json"
        clone.write_text((prepared / "seed.json").read_text().replace(str(prepared / "workspace"), str(root)))
        class Endpoint(BaseHTTPRequestHandler):
            def log_message(self, *args):
                pass
            def do_POST(self):
                body = json.loads(self.rfile.read(int(self.headers["Content-Length"])))
                wires.append("coder" if body.get("tools") else "observer")
                if len(wires) > 1:
                    self.send_error(400, "local control stops after the formatting action")
                    return
                args = json.loads(call["function"]["arguments"])
                args["cwd"] = str(root)
                delta = dict(content=message.get("content",""), tool_calls=[dict(index=0, id="replayed-format-action",
                    type="function", function=dict(name="exec",arguments=json.dumps(args)))])
                data = ("data: "+json.dumps({"choices":[{"delta":delta,"finish_reason":"stop"}]})+"\n\ndata: [DONE]\n\n").encode()
                self.send_response(200);self.send_header("Content-Type","text/event-stream")
                self.send_header("Content-Length",str(len(data)));self.end_headers();self.wfile.write(data)
        with ThreadingHTTPServer(("127.0.0.1",0),Endpoint) as srv:
            th=threading.Thread(target=srv.serve_forever,daemon=True);th.start()
            try:
                command=[binary,"--provider","openai","--model","gpt-5.6-luna","--base-url",f"http://127.0.0.1:{srv.server_port}/v1",
                         "--api-key","fixture","--timeout-seconds","5","coupled-run","--tool-root",root,"--seed-session",clone,
                         "--prompt-file",prepared/"task.txt","--out-dir",area/"run","--without-observer","--max-requests","8"]
                subprocess.run(command,cwd=root,env=execution_env(area,target),timeout=60,capture_output=True)
            finally:
                srv.shutdown();th.join()
        r=read_json(area/"run/report.json")
        acts=[x for x in r["execution"]["actions"] if x["call_id"]=="replayed-format-action"]
        assert len(acts)==1 and wires[0]=="coder"
        action=acts[0]
        before=read_json(prepared/"initial-inventory.json"); after=inventory(root)
        return dict(binary_sha256=sha(binary),seed_sha256=sha(prepared/"seed.json"),original_call=call["id"],
                    attempted_command="cargo fmt --all",executed=action["executed"],succeeded=action["succeeded"],
                    native_result=(action["result"] or "").replace(str(root), "<workspace>").replace(str(root.resolve()), "<workspace>")[:630],
                    wires=wires,external_requests=0,
                    actual_changed_paths=sorted(k for k in before.keys()|after.keys() if before.get(k)!=after.get(k)),
                    formatted_regression_sha256=sha(root/"tests/tui_replay_catalog.rs"))


if __name__=="__main__":
    print(json.dumps(probe(*(Path(x).resolve() for x in sys.argv[1:])),indent=2))
