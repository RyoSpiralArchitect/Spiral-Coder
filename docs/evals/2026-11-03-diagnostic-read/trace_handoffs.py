"""Bind hosted diagnostic starts, authored envelopes and subsequent native reads."""
import argparse
from collections import Counter
import hashlib
import json
from pathlib import Path
import sys

sys.path.insert(0, str(Path(__file__).resolve().parents[3]/"scripts"))
from coupled_bench.common import read_json, sha
from self_dogfood.evidence import bound_run


def unique(pairs):
    result={}
    for key,value in pairs:
        if key in result:
            raise ValueError(f"duplicate field {key}")
        result[key]=value
    return result


def measure(root, pin, receipt):
    if sha(root/"predeclared.json") != pin or not bound_run(root,pin,receipt):
        raise ValueError("sealed run required")
    recording = root/"episode/run"
    report=read_json(recording/"report.json")
    rows=report["work"]["requests"]
    events=[json.loads(line) for line in (recording/"trace.jsonl").read_bytes().splitlines()]
    starts=[row["data"] for row in events if row["event"]=="request_started"]
    if len(starts)!=len(rows):
        raise ValueError("cannot align native starts and durable wire receipts")
    attempts=[dict(number=i+1, format=start.get("response_format"), status=row["status"]) for i,(start,row) in enumerate(zip(starts,rows)) if row["role"]=="observer"]
    replies=[m for m in report["observer_display"] if m["role"]=="Assistant"]
    assists={a["assist_id"]:a for a in report["work"]["assists"]}
    actions={a["call_id"]:a for a in report["execution"]["actions"]}
    # Never pair replies by position if a retry, interrupted request, or HTTP
    # failure could introduce a wire attempt without a corresponding reply.
    aligned=len(attempts)==len(replies) and all(a["status"] in ("completed","output_limited") for a in attempts)
    results=[]
    for idx,attempt in enumerate(attempts):
        if attempt["format"]!="json_schema": continue
        item=dict(attempt)
        if aligned:
            reply=replies[idx]
            raw=reply.get("raw_content") or reply["content"]
            item["raw_sha256"]=hashlib.sha256(raw.encode()).hexdigest()
            try:
                parsed=json.loads(raw,object_pairs_hook=unique)
            except ValueError: parsed=None
            suggestions=(parsed or {}).get("suggestions",[]) if isinstance(parsed,dict) else []
            action=suggestions[0] if len(suggestions)==1 and isinstance(suggestions[0],dict) else {}
            item["single_response_contract"]=isinstance(parsed,dict) and isinstance(parsed.get("response_contract"),dict)
            item["single_suggestion"]=len(suggestions)==1
            item["native_validated"]=reply.get("raw_content") is not None
            item["suggested_tool"]=action.get("suggested_tool")
            item["authored_args"]=action.get("suggested_args")
            assist=assists.get(idx+1,{})
            execution=assist.get("suggestion_execution") or {}
            matches=[]
            if execution.get("executed") and execution.get("tool")==item["suggested_tool"]=="read_file" and item["authored_args"]==execution.get("arguments"):
                for cid in execution.get("matching_call_ids",[]):
                    logged=actions.get(cid,{})
                    if logged.get("executed") is True and logged.get("arguments")==item["authored_args"] and logged.get("tool")=="read_file" and (logged.get("result") or "").startswith("["+item["authored_args"]["path"]+"] ("):
                        matches.append({"call_id":cid,"step":logged["result_step"],"header":logged["result"].splitlines()[0]})
            item.update(assist_decision=assist.get("decision"),matched_native_reads=matches)
        results.append(item)
    return {"declaration_sha256":pin,"run_receipt_sha256":receipt,"wire_starts":len(starts),
            "response_formats":dict(Counter(a["format"] or "unspecified" for a in attempts)),
            "reply_alignment_unambiguous":aligned,"diagnostic_attempts":results,
            "scoped_rust_lookups":[{"arguments":a["arguments"],"executed":a["executed"],"result_header":(a.get("result") or "").splitlines()[:2]}
                for a in report["execution"]["actions"] if a["tool"]=="search_files"]}


if __name__ == "__main__":
    p=argparse.ArgumentParser(description=__doc__)
    p.add_argument("episode",type=Path);p.add_argument("--declaration",required=True);p.add_argument("--receipt",required=True)
    args=p.parse_args()
    print(json.dumps(measure(args.episode,args.declaration,args.receipt),indent=2))
