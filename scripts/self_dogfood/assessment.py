"""Run caller-frozen checks on copies assembled from original source and allowed edits."""
import json
import re
from pathlib import Path
from urllib.parse import urlsplit
import shutil
import tempfile
import time

from coupled_bench.common import PROVIDERS, inventory, read_json, sha, write_json
from .contracts import apply_overlays, execution_env, load_frozen, now
from .process import run_process
from .evidence import bound_run


def checks(out, spec, label, overlays=(), candidate=None):
    target_dir = Path(read_json(out / "layout.json")["target_dir"])
    results = []
    deadline = time.monotonic() + spec["evaluation_wall_seconds"]
    built_binary = None
    receipts = out / "checks" / label / "artifacts"
    receipts.mkdir(parents=True, exist_ok=True)
    with tempfile.TemporaryDirectory(prefix=f"spiral-self-{label}-", dir=out / "scratch") as temporary:
        workspace = Path(temporary) / "workspace"
        shutil.copytree(out / "baseline", workspace)
        if candidate is not None:
            for name in spec["editable"]:
                source, target = candidate / name, workspace / name
                if source.is_symlink() or any(p.is_symlink() for p in source.parents if p != candidate and candidate in p.parents):
                    raise ValueError("Candidate editable artifact is a symlink")
                if source.is_file():
                    target.parent.mkdir(parents=True, exist_ok=True)
                    shutil.copy2(source, target)
                elif target.exists():
                    target.unlink()
        apply_overlays(workspace, out / "inputs", overlays)
        apply_overlays(workspace, out / "inputs", spec["probes"])
        for check in spec["checks"]:
            replacements = {"workspace": str(workspace), "target_dir": str(target_dir),
                            "runtime": str(out / "runtime/spiral-coder"), "receipts": str(receipts)}
            argv = [arg.format_map(replacements) for arg in check["argv"]]
            remaining = deadline - time.monotonic()
            log = out / "checks" / label / (check["id"] + ".log")
            requires_binary = check["argv"][0] == "{target_dir}/debug/spiral-coder"
            if remaining <= 0 or (requires_binary and built_binary is None):
                result = {"exit_code": None, "timed_out": True, "output_limit": False, "wall_seconds": 0}
            else:
                result = run_process(argv, workspace, execution_env(out, target_dir),
                                     min(remaining, check["timeout_seconds"]), log)
            binary = target_dir / "debug" / ("spiral-coder.exe" if __import__("os").name == "nt" else "spiral-coder")
            content = log.read_text(errors="replace") if log.exists() else ""
            expected_count = check.get("expected_test_count")
            summaries = re.findall(r"test result: (ok|FAILED)\. (\d+) passed; (\d+) failed; (\d+) ignored;", content)
            expected_failures = check.get("expected_baseline_failures", 1)
            pass_summary = ("ok", str(expected_count), "0", "0")
            fail_summary = ("FAILED", str(expected_count - expected_failures), str(expected_failures), "0") if expected_count else None
            final_summary = summaries[-1] if summaries else None
            count_matches = expected_count is None or pass_summary == final_summary
            baseline_count_matches = expected_count is None or fail_summary == final_summary
            minimum = check.get("minimum_passed_tests")
            minimum_matches = minimum is None or (final_summary is not None and final_summary[0] == "ok" and int(final_summary[1]) >= minimum and final_summary[2] == "0")
            count_matches = count_matches and minimum_matches
            failure_matches = all(text in content for text in check.get("baseline_failure_contains", []))
            passed = result["exit_code"] == 0 and not result["timed_out"] and not result["output_limit"] and count_matches
            if check["kind"] == "build":
                built_binary = sha(binary) if passed and binary.is_file() else None
            result.update({"expected_test_count_matches": count_matches, "baseline_test_count_matches": baseline_count_matches, "baseline_failure_matches": failure_matches,
                           "id": check["id"], "kind": check["kind"], "argv": argv,
                           "passed": passed, "log_sha256": sha(log) if log.exists() else None,
                           "log": str(log.relative_to(out)),
                           "candidate_binary_sha256": built_binary})
            results.append(result)
    return results


def manifest(out, assessment_path):
    paths = {}
    # Cargo caches can be shared and are not receipts; workspace scope stays strict.
    for name in ["predeclared.json", "predeclared.sha256", "layout.json", "preflight.json",
                 "runtime", "driver", "inputs", "episode", "checks", "assessment-started.json", "run-receipt.json", assessment_path.name]:
        path = out / name
        if path.is_file():
            paths[name] = {"sha256": sha(path), "bytes": path.stat().st_size}
        elif path.is_dir():
            paths.update({f"{name}/{k}": v for k, v in inventory(path).items()})
    return {"assessment_sha256": sha(assessment_path), "files": paths,
            "excluded_operational_paths": ["scratch", "build cache from layout.json"],
            "workspace_scope_exemptions": [".git internals only"]}


def assess(args):
    out = args.out.resolve()
    declaration, spec = load_frozen(out, args.declaration_sha256)
    bound = bound_run(out, args.declaration_sha256, args.run_sha256)
    if not (out / "episode/started.json").exists():
        raise ValueError("Episode has not started")
    if any(out.glob("assessment-*.json")) or (out / "assessment-started.json").exists():
        raise ValueError("Assessment already exists; preserve it instead of re-running checks")
    with (out / "assessment-started.json").open("x") as handle:
        json.dump({"started_at": now(), "predeclaration_sha256": args.declaration_sha256}, handle)
    candidate = out / "episode/workspace"
    before, after = declaration["baseline_inventory"], inventory(candidate)
    changed = sorted(p for p in set(before) | set(after) if before.get(p) != after.get(p))
    forbidden = [p for p in changed if p not in spec["editable"]]
    native = out / "episode/run/report.json"
    ledger = out / "episode/run/state/tui-work.json"
    report = read_json(native) if native.exists() else {"completed": False, "work": read_json(ledger) if ledger.exists() else None}
    work = report.get("work") or {}
    durable = read_json(ledger) if ledger.exists() else None
    ledger_match = durable is not None and all(work.get(k) == durable.get(k) for k in ("task_id", "root", "tool_root", "request_limit", "requests"))
    work_match = work.get("root") == (out / "episode/task.txt").read_text().strip() and work.get("tool_root") == str(candidate) and bool(work.get("task_id"))
    process_path = out / "episode/process.json"
    process = read_json(process_path) if process_path.exists() else {}
    requests = (report.get("work") or {}).get("requests")
    roles = {r: sum(x.get("role") == r for x in requests) for r in ("coder", "observer")} if isinstance(requests, list) else {}
    expected_roles = {}
    for name, role in spec["roles"].items():
        url = urlsplit(PROVIDERS[role["provider"]][1])
        expected_roles[name] = {"provider": role["provider"], "model": role["model"],
            "transport": role["provider"] if role["provider"] in ("mistral", "anthropic") else "openai-compatible",
            "endpoint_origin": f"{url.scheme}://{url.netloc}",
            **{k: spec[k] for k in ("temperature", "max_tokens", "timeout_seconds")}}
    expected_roles["observer"].update(temperature=0.2, max_tokens=min(spec["max_tokens"], 1200))
    role_match = report.get("roles") == expected_roles
    actions = (report.get("execution") or {}).get("actions", [])
    edits = [a for a in actions if a.get("tool") in ("write_file", "patch_file", "apply_diff") and a.get("executed") is True and a.get("succeeded") is True]
    last_mutation = max([(report.get("execution") or {}).get("mutation_step") or 0] + [a.get("result_step") or 0 for a in edits])
    fresh = {command: any(a.get("tool") == "exec" and a.get("executed") is True and a.get("succeeded") is True
                          and (a.get("arguments") or {}).get("command") == command
                          and (a.get("result_step") or 0) > last_mutation for a in actions) for command in spec["native_checks"]}
    done = [a for a in actions if a.get("tool") == "done" and a.get("executed") is True and a.get("result") == "OK: done"]
    acceptance = bool(done) and (done[-1].get("arguments") or {}).get("remaining_acceptance", []) == []
    independent = checks(out, spec, "candidate", candidate=candidate)
    # Checks are outside candidate, and may not rewrite frozen authority either.
    load_frozen(out, args.declaration_sha256)
    bound_run(out, args.declaration_sha256, args.run_sha256)
    candidate_unchanged = inventory(candidate) == after
    gates = {"pinned_run_receipt": bound, "native_ledger_binding": ledger_match and work_match, "native_completed": report.get("completed") is True, "accepted_done": bool(done),
             "no_remaining_acceptance": acceptance, "native_edit": bool(edits), "actual_edit": bool(changed),
             "fresh_native_checks": all(fresh.values()), "no_pending_checks": (report.get("execution") or {}).get("pending_exact_checks") == [],
             "strict_scope": not forbidden, "original_protected_files_preserved": all(before.get(p) == after.get(p) for p in declaration["protected_paths"]),
             "both_roles_called": all(roles.get(r, 0) > 0 for r in ("coder", "observer")),
             "exact_role_configuration": role_match and report.get("same_model_both_roles") is False and report.get("coder_chunk_requests") == spec["coder_chunk_requests"],
             "role_accounting_consistent": report.get("role_requests") == roles and (report.get("work") or {}).get("request_limit") == spec["max_requests"],
             "bounded_requests": isinstance(requests, list) and 0 < len(requests) <= spec["max_requests"],
             "process_ok": process.get("exit_code") == 0 and not process.get("timed_out") and not process.get("output_limit"),
             "independent_checks_passed": all(c["passed"] for c in independent), "candidate_unchanged_by_assessor": candidate_unchanged,
             "no_secret_redactions": process.get("redacted_files") == [] and process.get("log_redactions") == 0, "patch_retained": process.get("patch_exit_code") == 0}
    target = out / "assessment-001.json"
    result = {"version": 1, "assessed_at": now(), "predeclaration_sha256": args.declaration_sha256, "run_receipt_sha256": args.run_sha256,
              "baseline_commit": declaration["baseline_commit"], "runtime_commit": declaration["runtime_commit"],
              "qualified_candidate": all(gates.values()), "promotion_authorized": False, "gates": gates,
              "changed_files": changed, "out_of_scope_files": forbidden, "native_checks": fresh,
              "requests": len(requests) if isinstance(requests, list) else None, "request_roles": roles,
              "process": process, "independent_checks": independent,
              "candidate_inventory": after, "roles": spec["roles"], "native_roles": report.get("roles")}
    write_json(target, result)
    write_json(out / "raw-manifest-001.json", manifest(out, target))
    return {"assessment": str(target), "qualified_candidate": result["qualified_candidate"], "gates": gates,
            "requests": result["requests"], "request_roles": roles}
