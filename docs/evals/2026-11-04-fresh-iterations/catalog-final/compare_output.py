"""Measure both public verifiers and send the quiet check through the real native exec consumer."""
import json
from http.server import BaseHTTPRequestHandler, ThreadingHTTPServer
from pathlib import Path
import shutil
import subprocess
import sys
import tempfile
import threading

from next_inputs import prior, QUIET_COMMAND
REPO = Path(__file__).resolve().parents[3]
sys.path.insert(0, str(REPO / "scripts"))
from coupled_bench.common import inventory, read_json, sha
from self_dogfood.build_cache import reset_package_cache
from self_dogfood.contracts import execution_env
from self_dogfood.runner import cache_lock
import shlex


def native(binary, workspace, command, env, out):
    task = out.parent / "verify.txt"
    prompt = ("Inspect src/tui_replay/catalog.rs and verify empty selection on this workspace using the exact command below. "
              "Keep all files unchanged. Report the actual outcome.\nRun exactly: " + command)
    task.write_text(prompt)
    seed = out.parent / "verification-session.json"
    goal = "Verify empty selection using the exact requested comparison"
    seed.write_text(json.dumps(dict(version=1, created_at_ms=1, updated_at_ms=1, tool_root=str(workspace), messages=[
        {"role":"user","content":prompt},
        {"role":"assistant","accepted_plan":{"goal":goal,"steps":["Inspect the catalog","Run the comparison","Report the result"],
         "acceptance_criteria":["The exact comparison was executed"],"risks":"a false report","assumptions":"the candidate is present"},
         "tool_calls":[{"id":"inspected","type":"function","function":{"name":"read_file","arguments":json.dumps({"path":"src/tui_replay/catalog.rs"})}}]},
        {"role":"tool","tool_call_id":"inspected","content":"[src/tui_replay/catalog.rs] (bounded inspection)\n"+(workspace/"src/tui_replay/catalog.rs").read_text()}])))
    n = []
    class Endpoint(BaseHTTPRequestHandler):
        def log_message(self,*args):
            pass
        def do_POST(self):
            self.rfile.read(int(self.headers["Content-Length"])); n.append(1)
            if len(n) > 1:
                self.send_error(400,"control ends after the comparison"); return
            args=json.dumps({"command":command})
            text=("<plan>\ngoal: "+goal+"\nsteps: 1) Inspect the catalog. 2) Run the exact comparison. "
                  "3) Report the actual result.\nacceptance: 1) The exact comparison was executed.\nrisks: a false report\n"
                  "assumptions: the candidate is present\n</plan>\n<think>\ngoal: Verify the requested CLI behavior\nstep: 2\n"
                  "tool: exec\nrisk: unexpected exit\ndoubt: output agreement\nnext: run the exact check\nverify: observe the native result\n</think>")
            delta=dict(content=text,tool_calls=[dict(index=0,id="public-check",type="function",function=dict(name="exec",arguments=args))])
            b=("data: "+json.dumps({"choices":[{"delta":delta,"finish_reason":"stop"}]})+"\n\ndata: [DONE]\n\n").encode()
            self.send_response(200);self.send_header("Content-Type","text/event-stream")
            self.send_header("Content-Length",str(len(b)));self.end_headers();self.wfile.write(b)
    with ThreadingHTTPServer(("127.0.0.1",0),Endpoint) as srv:
        th=threading.Thread(target=srv.serve_forever,daemon=True);th.start()
        try:
            argv=[binary,"--provider","openai","--model","offline","--api-key","fixture","--base-url",
                  f"http://127.0.0.1:{srv.server_port}/v1","--timeout-seconds","20","coupled-run","--without-observer",
                  "--tool-root",workspace,"--out-dir",out,"--prompt-file",task,"--seed-session",seed,"--max-requests","6"]
            proc=subprocess.run(argv,cwd=workspace,env=env,capture_output=True,timeout=90)
        finally:
            srv.shutdown();th.join()
    report=read_json(out/"report.json")
    x=next(x for x in report["execution"]["actions"] if x["call_id"]=="public-check")
    return dict(executed=x["executed"],succeeded=x["succeeded"],native_result=x["result"].splitlines()[0],
                purposefully_stopped_after_action=proc.returncode!=0,external_requests=0)


def compare(r19, r21, binary, target):
    rows=[]
    with tempfile.TemporaryDirectory(prefix="spiral-probe-output-") as temp, cache_lock(target):
        area=Path(temp).resolve()
        env=execution_env(area,target,binary)
        for label,source in (("r19_mismatch",r19),("r21_match",r21)):
            root=area/label
            shutil.copytree(source/"episode/workspace",root,ignore=shutil.ignore_patterns(".git"))
            assert inventory(root)==read_json(source/"episode/candidate-inventory.json")
            reset_package_cache(area,root,target,area/(label+"-reset.json"))
            measurements=[]
            for name,command in (("original",prior["PUBLIC_COMMANDS"][0]),("quiet",QUIET_COMMAND)):
                p=subprocess.run(shlex.split(command),cwd=root,env=env,capture_output=True,text=True,timeout=120)
                measurements.append(dict(form=name,exit=p.returncode,stdout=p.stdout[-2100:],stderr=p.stderr[-2300:]))
            assert [v["exit"]==0 for v in measurements]==[label=="r21_match"]*2
            actual=native(binary,root,QUIET_COMMAND,env,area/(label+"-native"))
            assert actual["executed"] and actual["succeeded"]==(label=="r21_match"), (label,actual)
            rows.append(dict(candidate=label,seal_sha256=sha(source/"run-receipt.json"),measurements=measurements,
                             native_quiet_check=actual))
    return dict(executor_sha256=sha(binary),cases=rows,external_requests=0)


if __name__=="__main__":
    print(json.dumps(compare(*(Path(x).resolve() for x in sys.argv[1:])),indent=2))
