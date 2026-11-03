"""Bind the preserved regression, four native checks and the external behavior to one candidate."""
import argparse
from collections import Counter
import json
from pathlib import Path
import sys

HERE = Path(__file__).resolve().parent
sys.path.insert(0, str(HERE.parents[3] / "scripts"))
from coupled_bench.common import inventory, read_json, sha


def audit(trial, external, parity_file, guide_file, process_file):
    plan = read_json(HERE / "plan.json")
    seal = read_json(trial / "receipt.json")
    ext, parity, guide = map(read_json, (external, parity_file, guide_file))
    digest = sha(trial / "receipt.json")
    assert digest == read_json(process_file)["receipt_sha256"] == ext["chain_receipt_sha256"]
    assert seal["plan_sha256"] == sha(HERE / "plan.json") and not seal["redacted_files"]
    assert sha(trial / "composite.patch") == seal["patch_sha256"]
    assert inventory(trial / "run") == seal["run_inventory"]
    source = trial / "workspace"
    assert inventory(source, include_nested_git=True) == seal["final_inventory"]
    before, after = read_json(trial / "initial-inventory.json"), seal["final_inventory"]
    delta = sorted(n for n in before.keys() | after.keys() if before.get(n) != after.get(n))
    manifest = read_json(trial / "run/iterations.json")
    reports, ownership, roles = [], [], Counter()
    task = (trial / "task.txt").read_text().strip()
    for i, child in enumerate(manifest["iterations"]):
        folder = trial / "run" / child["directory"]
        r = read_json(folder / "report.json")
        w = r["work"]
        ownership.append(w == read_json(folder / "state/tui-work.json") and
                         w["tool_root"] == str(source) and w["root"] == task and
                         w["task_id"] == child["task_id"] and
                         child["parent_task_id"] == (reports[-1]["work"]["task_id"] if i else None))
        roles.update(x["role"] for x in w["requests"])
        reports.append(r)
    final = reports[-1]
    spec = json.loads(task.split("<task_requirements>")[1].split("</task_requirements>")[0])
    expected = [c["command"] for r in spec["requirements"] for c in r["checks"]]
    acts = final["execution"]["actions"]
    last_edit = max([final["execution"]["mutation_step"] or 0] +
                    [x["result_step"] for x in acts if x["executed"] and (x["edit_outcome"] == "changed"
                     or (x["tool"] == "exec" and x["succeeded"] and x["arguments"].get("command") == "cargo fmt --all"))])
    done = [x for x in acts if x["tool"] == "done" and x["executed"] and x["result"] == "OK: done"]
    fresh = {cmd: [dict(step=x["result_step"], call_id=x["call_id"]) for x in acts if x["tool"] == "exec"
             and x["arguments"].get("command") == cmd and x["executed"] and x["succeeded"] and x["freshness"] == "current"
             and x["result_step"] > last_edit and done and x["result_step"] < done[-1]["result_step"]]
             for cmd in expected}
    gates = dict(pinned_workspace=True, child_ownership=all(ownership), both_roles=all(roles[k] for k in ("coder","observer")),
                 within_budget=manifest["task_requests"] <= plan["maximum_task_requests"] and manifest["wire_requests"] <= plan["maximum_wire_requests"],
                 original_baseline_replayed=ext["matched_original_tree_replay"], no_out_of_scope_changes=not ext["out_of_scope"],
                 changed_only_the_two_requested_files=delta == ["src/tui_replay/catalog.rs","tests/tui_replay_catalog.rs"],
                 regression_preserved=sha(source / "tests/tui_replay_catalog.rs") == plan["preserved_regression_sha256"],
                 source_and_docs_required_by_original_issue=not ext["missing_required"],
                 all_four_native_checks_after_final_edit=all(fresh.values()) and not final["execution"]["pending_exact_checks"],
                 native_report_accepted=bool(manifest["completed"] and done and not done[-1]["arguments"].get("remaining_acceptance")),
                 all_original_independent_groups=ext["independent_all_passed"], public_diagnostic_and_no_writes=parity["matches"],
                 packaged_guide=guide["matches"])
    return dict(plan_sha256=sha(HERE/"plan.json"), receipt_sha256=digest, candidate_patch_sha256=seal["patch_sha256"],
                original_task_during_this_followup=False, gates=gates, assisted_acceptance_passed=all(gates.values()),
                changed_in_this_followup=delta, composite_changed_paths=ext["changed_files"], source_regression_sha256=sha(source/"tests/tui_replay_catalog.rs"),
                manifest=manifest, role_wires=dict(roles), last_mutation=last_edit, exact_checks=fresh,
                terminal_reports=[dict(step=x["result_step"],outcome=x["result"],args=x["arguments"]) for x in acts if x["tool"]=="done"],
                independent=ext["independent_checks"], public_diagnostic_output_sha256=sha(parity_file),
                packaged_guide_output_sha256=sha(guide_file))


if __name__ == "__main__":
    p=argparse.ArgumentParser(description=__doc__)
    for k in ("trial","external","parity","guide","process"):
        p.add_argument(k,type=Path)
    a=p.parse_args()
    print(json.dumps(audit(*(getattr(a,k).resolve() for k in ("trial","external","parity","guide","process"))),indent=2))
