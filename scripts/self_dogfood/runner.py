"""Prepare one frozen mixed-family episode; retain failures without automatic adoption."""
from contextlib import contextmanager
import os
from pathlib import Path
import shutil
import subprocess
import time

from coupled_bench.common import PROVIDERS, inventory, read_json, sha, write_json
from .contracts import (copy_inputs, execution_env, freeze_driver, git_commit, git_snapshot,
                        load_frozen, now, validate)
from .assessment import assess, checks
from .process import run_process
from .evidence import seal_run


@contextmanager
def cache_lock(target, seconds=60):
    if os.name != "posix":
        raise ValueError("This first self-dogfood runner requires POSIX process groups and cache locks")
    import fcntl
    target.mkdir(parents=True, exist_ok=True)
    with (target / ".self-dogfood.lock").open("a") as handle:
        deadline = time.monotonic() + seconds
        while True:
            try:
                fcntl.flock(handle, fcntl.LOCK_EX | fcntl.LOCK_NB)
                break
            except BlockingIOError:
                if time.monotonic() >= deadline:
                    raise ValueError("Build cache is owned by another episode; no run started")
                time.sleep(0.1)
        try:
            yield
        finally:
            fcntl.flock(handle, fcntl.LOCK_UN)


def protected_files(baseline, spec):
    protected = set(spec.get("protected", []))
    for name in baseline:
        path = Path(name)
        if "tests" in path.parts or path.name.startswith("test_") or path.name.endswith(("_tests.rs", "_test.rs", ".test.mjs")):
            protected.add(name)
    if protected & set(spec["editable"]):
        raise ValueError("Existing standalone tests cannot be editable in this lane")
    if any(name not in baseline for name in protected):
        raise ValueError("Every protected path must exist in the frozen baseline")
    return sorted(protected)


def prepare(args):
    repo, spec_path, binary, out = (p.resolve() for p in (args.repo, args.spec, args.binary, args.out))
    if out.exists() or out.is_relative_to(repo):
        raise ValueError("Output must be fresh and outside the source repository")
    runtime_commit = git_commit(repo, args.runtime_commit)
    spec = read_json(spec_path)
    validate(spec)
    if not binary.is_file():
        raise ValueError("Build the execution runtime before preparation")
    target_dir = args.target_dir.resolve() if args.target_dir else out / "build"
    if target_dir.is_relative_to(repo) or any(target_dir.is_relative_to(out / p) for p in ["baseline", "inputs", "driver", "runtime", "episode"]):
        raise ValueError("Build cache must be outside source and frozen/workspace directories")
    out.mkdir(parents=True)
    (out / "scratch").mkdir(); (out / "runtime").mkdir(); (out / "episode").mkdir()
    baseline_commit = git_snapshot(repo, args.baseline_ref, out / "baseline")
    copy_inputs(spec_path, spec, out / "inputs")
    shutil.copy2(binary, out / "runtime/spiral-coder")
    baseline = inventory(out / "baseline")
    protected = protected_files(baseline, spec)
    shutil.copytree(out / "baseline", out / "episode/workspace")
    workspace = out / "episode/workspace"
    env = execution_env(out, target_dir)
    for command in [["git", "init", "-q"], ["git", "add", "."], ["git", "-c", "user.name=Self dogfood fixture", "-c", "user.email=fixture@localhost", "commit", "-qm", "Frozen candidate baseline"]]:
        subprocess.run(command, cwd=workspace, env=env, check=True, stdout=subprocess.PIPE, stderr=subprocess.PIPE)
    candidate_base = subprocess.check_output(["git", "rev-parse", "HEAD"], cwd=workspace, text=True).strip()
    shutil.copy2(out / "inputs" / spec["prompt"], out / "episode/task.txt")
    write_json(out / "layout.json", {"target_dir": str(target_dir), "source_repository": str(repo)})
    declaration = {"version": 1, "prepared_at": now(), "baseline_commit": baseline_commit,
                   "runtime_commit": runtime_commit, "candidate_base_commit": candidate_base, "binary_sha256": sha(out / "runtime/spiral-coder"),
                   "roles": spec["roles"], "same_model_both_roles": False,
                   "max_requests": spec["max_requests"], "wall_seconds": spec["wall_seconds"],
                   "baseline_inventory": baseline, "inputs_inventory": inventory(out / "inputs"),
                   "driver_sha256": freeze_driver(out), "protected_paths": protected,
                   "layout_sha256": sha(out / "layout.json"), "prompt_sha256": sha(out / "episode/task.txt"),
                   "executor_reloads_candidate": False, "promotion_authorized": False,
                   "boundary": "Frozen evaluator and separate copies, not an OS security sandbox"}
    write_json(out / "predeclared.json", declaration)
    (out / "predeclared.sha256").write_text(sha(out / "predeclared.json") + "\n")
    with cache_lock(target_dir):
        baseline_checks = checks(out, spec, "baseline")
        reference_checks = checks(out, spec, "reference", spec["reference"])
    expected = all((row["passed"] if check["baseline"] == "pass" else
                    row["exit_code"] not in (0, None) and not row["timed_out"] and not row["output_limit"]
                    and row["baseline_test_count_matches"] and row["baseline_failure_matches"])
                   for check, row in zip(spec["checks"], baseline_checks))
    preflight = {"prepared_at": now(), "baseline_expected": expected,
                 "reference_passed": all(row["passed"] for row in reference_checks),
                 "baseline": baseline_checks, "reference": reference_checks, "provider_calls": 0}
    write_json(out / "preflight.json", preflight)
    declaration["preflight_sha256"] = sha(out / "preflight.json")
    write_json(out / "predeclared.json", declaration)
    (out / "predeclared.sha256").write_text(sha(out / "predeclared.json") + "\n")
    if inventory(out / "baseline") != baseline or inventory(out / "inputs") != declaration["inputs_inventory"]:
        raise ValueError("Preflight checks changed frozen inputs")
    if not expected or not preflight["reference_passed"]:
        raise ValueError("Baseline/reference qualification failed; preflight retained, no provider call allowed")
    return {"prepared": str(out), "baseline_commit": baseline_commit, "runtime_commit": runtime_commit,
            "provider_calls": 0, "declaration_sha256": sha(out / "predeclared.json"), "frozen_driver": str(out / "driver/run_self_dogfood.py")}


def command_and_env(out, spec):
    coder, observer = (spec["roles"][r] for r in ("coder", "observer"))
    env = execution_env(out, Path(read_json(out / "layout.json")["target_dir"]), out / "runtime/spiral-coder")
    secrets = []
    for role in (coder, observer):
        key = PROVIDERS[role["provider"]][0]
        if not os.environ.get(key):
            raise ValueError(f"Missing {key}; provide credentials via environment")
        env[key] = os.environ[key]; secrets.append(os.environ[key])
    command = [str(out / "runtime/spiral-coder"), "--provider", coder["provider"], "--model", coder["model"],
               "--base-url", PROVIDERS[coder["provider"]][1], "--temperature", str(spec["temperature"]),
               "--max-tokens", str(spec["max_tokens"]), "--timeout-seconds", str(spec["timeout_seconds"]),
               "coupled-run", "--tool-root", str(out / "episode/workspace"), "--prompt-file", str(out / "episode/task.txt"),
               "--out-dir", str(out / "episode/run"), "--max-requests", str(spec["max_requests"]),
               "--coder-chunk-requests", str(spec["coder_chunk_requests"]),
               "--observer-provider", observer["provider"], "--observer-model", observer["model"],
               "--observer-base-url", PROVIDERS[observer["provider"]][1],
               "--observer-api-key-env", PROVIDERS[observer["provider"]][0]]
    return command, env, secrets


def run(args):
    out = args.out.resolve()
    declaration, spec = load_frozen(out, args.declaration_sha256)
    preflight = read_json(out / "preflight.json")
    if not preflight.get("baseline_expected") or not preflight.get("reference_passed"):
        raise ValueError("Baseline/reference preflight must pass before provider work")
    episode = out / "episode"
    if (episode / "started.json").exists() or (episode / "run").exists():
        raise ValueError("Episode already started; no retry in place")
    if inventory(episode / "workspace") != declaration["baseline_inventory"] or sha(episode / "task.txt") != declaration["prompt_sha256"]:
        raise ValueError("Candidate workspace or prompt changed before execution")
    command, env, secrets = command_and_env(out, spec)
    target = Path(read_json(out / "layout.json")["target_dir"])
    with cache_lock(target):
        with (episode / "started.json").open("x") as handle:
            __import__("json").dump({"started_at": now(), "command": command,
                                   "predeclaration_sha256": sha(out / "predeclared.json")}, handle, indent=2)
        result = run_process(command, episode / "workspace", env, spec["wall_seconds"], episode / "console.log", secrets)
    redacted = []
    for path in episode.rglob("*"):
        if path.is_file() and not path.is_symlink() and ".git" not in path.relative_to(episode).parts:
            body = path.read_bytes()
            replacement = body
            for secret in secrets:
                replacement = replacement.replace(secret.encode(), b"[REDACTED]")
            if replacement != body:
                path.write_bytes(replacement); redacted.append(path.relative_to(episode).as_posix())
    result.update({"finished_at": now(), "redacted_files": redacted})
    subprocess.run(["git", "add", "-N", "."], cwd=episode / "workspace", env=execution_env(out, target), stdout=subprocess.PIPE, stderr=subprocess.PIPE, timeout=30)
    patch = subprocess.run(["git", "diff", "--binary", declaration["candidate_base_commit"], "--"], cwd=episode / "workspace",
                           env=execution_env(out, target), stdout=subprocess.PIPE, stderr=subprocess.PIPE, timeout=30)
    (episode / "candidate.patch").write_bytes(patch.stdout)
    result["patch_exit_code"] = patch.returncode
    write_json(episode / "process.json", result)
    write_json(episode / "candidate-inventory.json", inventory(episode / "workspace"))
    load_frozen(out, args.declaration_sha256)
    receipt_digest = seal_run(out, args.declaration_sha256)
    return {"finished": str(out), "run_receipt_sha256": receipt_digest, **result}


def evaluate(args):
    out = args.out.resolve()
    load_frozen(out, args.declaration_sha256)
    with cache_lock(Path(read_json(out / "layout.json")["target_dir"])):
        return assess(args)
