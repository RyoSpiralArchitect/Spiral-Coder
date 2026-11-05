"""Reapply the final composite patch and run the original six #49 checks."""
import argparse
import hashlib
import json
from pathlib import Path
import shutil
import subprocess
import sys

HERE = Path(__file__).resolve().parent


def assess(raw, chain, result, dest, target, plan_path):
    plan=json.loads(plan_path.read_bytes())
    declaration=raw/"predeclared.json"
    digest=lambda path: hashlib.sha256(path.read_bytes()).hexdigest()
    assert digest(declaration)==plan["r18_declaration"]
    assert all(digest(raw/"driver"/name)==value for name,value in json.loads(declaration.read_bytes())["driver_sha256"].items())
    sys.path.insert(0,str(raw/"driver"))
    from coupled_bench.common import inventory, read_json, sha, write_json
    from self_dogfood.assessment import checks
    from self_dogfood.evidence import bound_run
    from self_dogfood.runner import cache_lock
    assert bound_run(raw,plan["r18_declaration"],plan["r18_seal"])
    stored=read_json(chain/"receipt.json")
    assert sha(chain/"receipt.json")==read_json(result)["receipt_sha256"] and stored["plan_sha256"]==sha(plan_path)
    assert not stored["redacted_files"] and not stored["process"]["log_redactions"]
    assert inventory(chain/"run")==stored["run_inventory"] and inventory(chain/"workspace",include_nested_git=True)==stored["final_inventory"]
    assert sha(chain/"composite.patch")==stored["patch_sha256"]
    dest.mkdir()
    decl=read_json(raw/"predeclared.json")
    for name in ("baseline","inputs"):
        shutil.copytree(raw/name,dest/name)
        assert inventory(dest/name)==decl[name+"_inventory"]
    a=dest/"checks/baseline/artifacts";a.mkdir(parents=True)
    shutil.copyfile(raw/"checks/baseline/artifacts/rust-tests.json",a/"rust-tests.json")
    assert sha(a/"rust-tests.json")=="03ee93445176ec84dfd9e87e64461b9151ccdf14f27e3934c488a48780240dc6"
    (dest/"scratch").mkdir();snapshot=dest/"replay";shutil.copytree(dest/"baseline",snapshot)
    if (chain/"composite.patch").stat().st_size:
        subprocess.run(["git","apply","--binary",chain/"composite.patch"],cwd=snapshot,check=True,capture_output=True)
    files=inventory(snapshot,include_nested_git=True)
    assert files==stored["final_inventory"]
    spec=read_json(dest/"inputs/spec.json")
    changed=sorted(n for n in files.keys()|decl["baseline_inventory"].keys() if files.get(n)!=decl["baseline_inventory"].get(n))
    write_json(dest/"layout.json",{"target_dir":str(target)})
    with cache_lock(target):
        verdicts=checks(dest,spec,"continuation",candidate=snapshot)
    manifest=read_json(chain/"run/iterations.json")
    rows=[]
    for step in manifest["iterations"]:
        report=read_json(chain/"run"/step["directory"]/"report.json")
        actions=report["execution"]["actions"]
        execs=[dict(step=v["result_step"],command=v["arguments"].get("command"),succeeded=v["succeeded"],freshness=v["freshness"])
               for v in actions if v["tool"]=="exec" and v["executed"] and v["arguments"].get("command") in spec["native_checks"]]
        latest=report["execution"]["mutation_step"] or 0
        done=[v for v in actions if v["tool"]=="done" and v.get("result")=="OK: done" and v["executed"]]
        steps=[v["result_step"] for v in done]
        fresh={cmd:any(v["command"]==cmd and v["step"]>latest and v["succeeded"] and v["freshness"]=="current" for v in execs) for cmd in spec["native_checks"]}
        edits=[dict(step=v["result_step"],path=v["arguments"].get("path"),id=v["call_id"]) for v in actions if v["edit_outcome"]=="changed" and v["executed"]]
        rows.append(dict(**step,role_requests=report["role_requests"],current_checks=fresh,exact_calls=execs,accepted_done=steps,
          latest_mutation=latest,guide_steps=[v["step"] for v in edits if v["path"]=="docs/evaluation.md"],changed_receipts=edits,
          remaining_native_checks=report["execution"]["pending_exact_checks"],
          terminal_summaries=[dict(step=v["result_step"],result=v.get("result"),args=v["arguments"]) for v in actions if v["tool"]=="done"]))
    final=rows[-1]
    native_closed=bool(final["completed"] and final["accepted_done"] and final["guide_steps"] and all(final["current_checks"].values())
                       and not final["remaining_native_checks"] and all(any(v["command"]==cmd and final["latest_mutation"]<v["step"]<final["accepted_done"][-1] and v["freshness"]=="current" and v["succeeded"] for v in final["exact_calls"]) for cmd in spec["native_checks"]))
    summary=dict(original_r18_seal=plan["r18_seal"],plan_sha256=sha(plan_path),chain_receipt_sha256=sha(chain/"receipt.json"),
        runtime_commit=plan["runtime_commit"],executor_sha256=plan["executor_sha256"],process=stored["process"],manifest=manifest,
        rounds=rows,changed_files=changed,out_of_scope=sorted(set(changed)-set(spec["editable"])),missing_required=sorted(set(spec["required_changed_paths"])-set(changed)),
        matched_original_tree_replay=True,composite_patch_sha256=stored["patch_sha256"],native_guide_checks_report=native_closed,
        independent_checks=[{k:c.get(k) for k in ("id","passed","exit_code","test_summary","log_sha256")} for c in verdicts],
        independent_all_passed=all(c["passed"] for c in verdicts),original_single_episode_qualification=None)
    write_json(dest/"assessment.json",summary)
    print(json.dumps({k:summary[k] for k in ("native_guide_checks_report","independent_all_passed","out_of_scope","missing_required","changed_files")}))


if __name__ == "__main__":
    p=argparse.ArgumentParser(description=__doc__)
    for name in ("raw","chain","result","dest","target","plan"):
        p.add_argument(name,type=Path)
    args=p.parse_args()
    assess(*(getattr(args,k).resolve() for k in ("raw","chain","result","dest","target","plan")))
