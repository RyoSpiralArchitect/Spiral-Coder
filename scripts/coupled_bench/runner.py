"""Freeze inputs before spending requests; retain every selected episode."""
from datetime import datetime, timezone
import json
import hashlib
import os
from pathlib import Path
import shutil
import subprocess
import time

from .common import (PROVIDERS, clean_env, inventory, read_json, run_process,
                     select, sha, validate, write_json)
from .evidence import assess
from .oracles import score


def now():
    return datetime.now(timezone.utc).isoformat()


def driver_inventory(root):
    return {p.relative_to(root).as_posix(): sha(p) for p in sorted(root.rglob("*.py"))
            if "__pycache__" not in p.parts}


def prepare(args):
    suite_path, matrix_path, binary, out = [p.resolve() for p in
                                          (args.suite, args.matrix, args.binary, args.out)]
    if out.exists():
        raise ValueError("Output exists; preserve it and choose a fresh directory")
    if out.is_relative_to(suite_path.parent):
        raise ValueError("Output must be outside the fixture source tree")
    suite, matrix = read_json(suite_path), read_json(matrix_path)
    validate(suite, matrix)
    cases, profiles = select(suite["cases"], args.case), select(matrix["profiles"], args.profile)
    if not binary.is_file():
        raise ValueError("Build the binary before preparing a benchmark")
    if any(p.is_symlink() for p in suite_path.parent.rglob("*")):
        raise ValueError("Fixture sources must not contain symlinks")
    out.mkdir(parents=True)
    (out / "runtime").mkdir()
    frozen_binary = out / "runtime" / ("spiral-coder.exe" if os.name == "nt" else "spiral-coder")
    shutil.copy2(binary, frozen_binary)
    shutil.copytree(suite_path.parent, out / "inputs")
    driver = Path(__file__).resolve().parent
    (out / "driver").mkdir()
    shutil.copytree(driver, out / "driver/coupled_bench", ignore=shutil.ignore_patterns("__pycache__"))
    shutil.copy2(driver.parent / "run_coupled_bench.py", out / "driver/run_coupled_bench.py")
    write_json(out / "matrix.json", matrix)
    write_json(out / "suite.json", suite)
    preflight = []
    for case in cases:
        original = out / "inputs" / case["workspace"]
        baseline = score(case, out / "inputs", original, frozen_binary)
        reference_dir = out / "references" / case["id"]
        shutil.copytree(original, reference_dir)
        shutil.copytree(out / "inputs" / case["reference"], reference_dir, dirs_exist_ok=True)
        reference = score(case, out / "inputs", reference_dir, frozen_binary)
        preflight.append({"id": case["id"], "baseline": baseline, "reference": reference})
        write_json(out / "preflight.json", preflight)
        if baseline["checks"].get("independent_check_0") or not reference["passed"]:
            raise ValueError(f"Invalid baseline/reference oracle for {case['id']}; retained preflight.json")
    episodes = []
    for profile in profiles:
        for case in cases:
            location = Path("episodes") / profile["id"] / case["id"]
            root = out / location / "workspace"
            shutil.copytree(out / "inputs" / case["workspace"], root)
            # Git status/diff is available to the actual harness, but never used as the file-scope oracle.
            for command in [["git", "init", "-q"], ["git", "add", "."],
                            ["git", "-c", "user.name=Benchmark fixture", "-c", "user.email=fixture@localhost",
                             "commit", "-qm", "Frozen benchmark baseline"]]:
                subprocess.run(command, cwd=root, env=clean_env(), check=True,
                               stdout=subprocess.PIPE, stderr=subprocess.PIPE)
            shutil.copy2(out / "inputs" / case["prompt"], out / location / "task.txt")
            episodes.append({"profile": profile["id"], "case": case["id"], "path": location.as_posix(),
                             "workspace_before": inventory(root), "prompt_sha256": sha(out / location / "task.txt")})
    declaration = {"version": 1, "created_at": now(), "runtime_commit": args.runtime_commit,
                   "binary_sha256": sha(frozen_binary), "binary_path": frozen_binary.relative_to(out).as_posix(),
                   "suite_sha256": sha(out / "suite.json"), "matrix_sha256": sha(out / "matrix.json"),
                   "input_inventory": inventory(out / "inputs"),
                   "driver_sha256": driver_inventory(out / "driver"),
                   "profiles": profiles, "cases": cases, "episodes": episodes,
                   "maximum_wire_requests": sum(c["max_requests"] for c in cases) * len(profiles),
                   "same_model_both_roles": True,
                   "scope_policy": "Retain every changed path except Git internals. Report behavior/handoff and strict all-file qualification separately; runtime/cache changes are not removed.",
                   "retry_policy": "One process per episode; no driver retries or fallback model. Native transport attempts consume the shared cap. A new trial requires a new output directory."}
    write_json(out / "predeclared.json", declaration)
    return {"prepared": str(out), "episodes": len(episodes),
            "maximum_wire_requests": declaration["maximum_wire_requests"], "provider_calls": 0,
            "frozen_driver": str(out / "driver/run_coupled_bench.py")}


def load_frozen(out, *, require_driver=True):
    declaration = read_json(out / "predeclared.json")
    if sha(out / declaration["binary_path"]) != declaration["binary_sha256"]:
        raise ValueError("Frozen runtime binary changed")
    for name in ["suite", "matrix"]:
        if sha(out / f"{name}.json") != declaration[f"{name}_sha256"]:
            raise ValueError(f"Frozen {name} changed")
    if inventory(out / "inputs") != declaration["input_inventory"]:
        raise ValueError("Frozen fixture/oracle inputs changed")
    current_driver = Path(__file__).resolve().parent.parent
    if require_driver and driver_inventory(current_driver / "coupled_bench") != {
            k.removeprefix("coupled_bench/"): v for k, v in declaration["driver_sha256"].items()
            if k.startswith("coupled_bench/")}:
        raise ValueError("Driver changed; use the frozen driver recorded by prepare")
    if require_driver and sha(current_driver / "run_coupled_bench.py") != declaration["driver_sha256"]["run_coupled_bench.py"]:
        raise ValueError("CLI changed; use the frozen driver")
    return declaration


def run(args):
    out = args.out.resolve()
    declaration = load_frozen(out)
    matrix = read_json(out / "matrix.json")
    profiles = select(declaration["profiles"], args.profile)
    cases = select(declaration["cases"], args.case)
    selected = [(p, c) for p in profiles for c in cases]
    # Validate every selected episode and key before executing any one of them.
    for profile, case in selected:
        folder = out / "episodes" / profile["id"] / case["id"]
        if (folder / "started.json").exists() or (folder / "run").exists():
            raise ValueError(f"Episode already started: {profile['id']}/{case['id']}; no retry in place")
        key_name = PROVIDERS[profile["provider"]][0]
        if not os.environ.get(key_name):
            raise ValueError(f"Missing {key_name}; credentials must come from the environment")
        episode = next(e for e in declaration["episodes"] if e["profile"] == profile["id"] and e["case"] == case["id"])
        if inventory(folder / "workspace") != episode["workspace_before"] or sha(folder / "task.txt") != episode["prompt_sha256"]:
            raise ValueError("Workspace or prompt changed before execution")
    for profile, case in selected:
        folder = out / "episodes" / profile["id"] / case["id"]
        binary = out / declaration["binary_path"]
        key_name, base_url = PROVIDERS[profile["provider"]]
        secret = os.environ[key_name]
        env = clean_env(binary)
        env[key_name] = secret
        command = [str(binary), "--provider", profile["provider"], "--base-url", base_url,
                   "--model", profile["model"], "--temperature", str(matrix["temperature"]),
                   "--max-tokens", str(matrix["max_tokens"]), "--timeout-seconds", str(matrix["timeout_seconds"]),
                   "coupled-run", "--tool-root", str(folder / "workspace"), "--prompt-file", str(folder / "task.txt"),
                   "--out-dir", str(folder / "run"), "--max-requests", str(case["max_requests"])]
        with (folder / "started.json").open("x") as handle:
            json.dump({"started_at": now(), "command": command, "credential_env_name": key_name}, handle, indent=2)
        print(json.dumps({"started": f"{profile['id']}/{case['id']}", "max_requests": case["max_requests"]}), flush=True)
        started = time.monotonic()
        code, log, expired = run_process(command, folder / "workspace", env, matrix["wall_seconds"])
        (folder / "console.log").write_text(log.replace(secret, "[REDACTED]"))
        redactions = []
        for path in (folder / "run").rglob("*"):
            if path.is_file() and secret.encode() in path.read_bytes():
                path.write_bytes(path.read_bytes().replace(secret.encode(), b"[REDACTED]"))
                redactions.append(path.relative_to(folder).as_posix())
        result = {"profile": profile["id"], "model": profile["model"], "case": case["id"],
                  "exit_code": code, "timed_out": expired, "wall_seconds": round(time.monotonic() - started, 3),
                  "finished_at": now(), "redacted_raw_files": redactions}
        write_json(folder / "process.json", result)
        print(json.dumps(result), flush=True)


def retained_report(folder):
    report_path = folder / "run/report.json"
    if report_path.exists():
        return read_json(report_path), "native_report"
    ledger = folder / "run/state/tui-work.json"
    if not ledger.exists():
        ledger = folder / "workspace/.spiral-coder/tui-work.json"
    # A failed first stream can exit older runtimes before their report writer.
    # Recover request accounting only, never synthesize native completion/proof.
    report = {"completed": False, "execution": None,
              "work": read_json(ledger) if ledger.exists() else None}
    return report, "durable_request_ledger_only" if ledger.exists() else "missing"


def summarize(args):
    out = args.out.resolve()
    declaration = load_frozen(out, require_driver=not args.reassess)
    current_driver = Path(__file__).resolve().parent.parent
    assessor = {f"coupled_bench/{k}": v for k, v in driver_inventory(current_driver / "coupled_bench").items()}
    assessor["run_coupled_bench.py"] = sha(current_driver / "run_coupled_bench.py")
    assessor_id = hashlib.sha256(json.dumps(assessor, sort_keys=True).encode()).hexdigest()
    snapshot = out / "assessors" / assessor_id
    if not snapshot.exists():
        snapshot.mkdir(parents=True)
        shutil.copytree(current_driver / "coupled_bench", snapshot / "coupled_bench", ignore=shutil.ignore_patterns("__pycache__"))
        shutil.copy2(current_driver / "run_coupled_bench.py", snapshot / "run_coupled_bench.py")
    rows = []
    for episode in declaration["episodes"]:
        folder = out / episode["path"]
        profile = next(p for p in declaration["profiles"] if p["id"] == episode["profile"])
        case = next(c for c in declaration["cases"] if c["id"] == episode["case"])
        process_file = folder / "process.json"
        process = read_json(process_file) if process_file.exists() else {}
        report, receipt_source = retained_report(folder)
        errors, pre_tool_rejections = [], []
        trace = folder / "run/trace.jsonl"
        if trace.exists():
            for line in trace.read_text().splitlines():
                try:
                    event = json.loads(line)
                except ValueError:
                    continue
                if event.get("event") == "error":
                    errors.append(event.get("data"))
                elif event.get("event") == "pre_tool_gate_rejected":
                    pre_tool_rejections.append(event.get("data"))
        state = "finished" if process else ("started_without_receipt" if (folder / "started.json").exists() else "not_run")
        oracle = score(case, out / "inputs", folder / "workspace", out / declaration["binary_path"])
        rows.append({"profile": profile, "case": case["id"], "state": state, "process": process,
                     "receipt_source": receipt_source, "trace_errors": errors,
                     "pre_tool_rejections": pre_tool_rejections,
                     **assess(case, episode["workspace_before"], folder / "workspace", report, process, oracle)})
    result = {"version": 1, "evaluated_at": now(), "predeclaration_sha256": sha(out / "predeclared.json"),
              "assessor_driver_sha256": assessor, "assessor_snapshot": snapshot.relative_to(out).as_posix(),
              "matches_declared_driver": assessor == declaration["driver_sha256"],
              "rows": rows, "counts": {"declared": len(rows), "finished": sum(r["state"] == "finished" for r in rows),
              "runtime_completed": sum(r["runtime_completed"] for r in rows),
              "independent_artifacts_passed": sum(r["independent_artifacts_passed"] for r in rows),
              "behavior_and_handoff_qualified": sum(r["behavior_and_handoff_qualified"] for r in rows),
              "strict_all_files_qualified": sum(r["strict_all_files_qualified"] for r in rows),
              "wire_requests": sum(r["requests"] or 0 for r in rows),
              "episodes_with_unknown_request_count": sum(r["requests"] is None for r in rows)}}
    number = 1
    while (out / f"assessment-{number:03d}.json").exists():
        number += 1
    target = out / f"assessment-{number:03d}.json"
    write_json(target, result)
    # The raw manifest names itself only through the immutable assessment digest.
    manifest = {"local_root": str(out), "assessment_sha256": sha(target), "files": inventory(out)}
    write_json(out / f"raw-manifest-{number:03d}.json", manifest)
    return {"assessment": str(target), **result["counts"]}
