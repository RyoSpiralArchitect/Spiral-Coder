"""Bounded integrated pilot. Supply a key through the environment, never argv.

Fixtures and baseline assertions are copied before execution; the independent
oracle runs outside the model's workspace. Previous output is never overwritten.
"""
import argparse
import hashlib
import json
import os
from pathlib import Path
import shutil
import signal
import subprocess
import time


def preserved(baseline, candidate):
    old = {c["id"]: c for c in baseline["cases"]}
    new = {c["id"]: c for c in candidate["cases"]}
    return (len(old) == len(baseline["cases"]) and len(new) == len(candidate["cases"])
            and all(new.get(k) == v for k, v in old.items())
            and all(candidate.get(k) == v for k, v in baseline.items() if k != "cases"))


def run_bounded(command, *, cwd, env, timeout):
    """Keep failed/timeout oracle receipts and stop the whole process group."""
    try:
        process = subprocess.Popen(command, cwd=cwd, env=env, stdout=subprocess.PIPE,
                                   stderr=subprocess.STDOUT, text=True, start_new_session=True)
    except OSError as error:
        return -1, str(error), False
    timed_out = False
    try:
        log, _ = process.communicate(timeout=timeout)
    except subprocess.TimeoutExpired:
        timed_out = True
        try:
            os.killpg(process.pid, signal.SIGTERM)
        except ProcessLookupError:
            pass
        try:
            log, _ = process.communicate(timeout=15)
        except subprocess.TimeoutExpired:
            try:
                os.killpg(process.pid, signal.SIGKILL)
            except ProcessLookupError:
                pass
            log, _ = process.communicate()
    return process.returncode, log, timed_out


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--binary", required=True, type=Path)
    parser.add_argument("--repo", required=True, type=Path)
    parser.add_argument("--out", required=True, type=Path)
    parser.add_argument("--provider", required=True, choices=["openai", "mistral", "gemini"])
    parser.add_argument("--model", required=True)
    parser.add_argument("--wall-seconds", type=int, default=600)
    parser.add_argument("--without-observer", action="store_true")
    parser.add_argument("--case", action="append", default=[], metavar="CASE_ID",
                        help="Run only these cases, in suite order; repeat to select more (duplicates ignored)")
    args = parser.parse_args()
    binary, repo, out = [p.resolve() for p in (args.binary, args.repo, args.out)]
    if out.exists():
        raise SystemExit("Output exists; preserve receipts and choose a new directory")
    spec_bytes = (repo / "tests/fixtures/coupled-loop-v1.json").read_bytes()
    cases = json.loads(spec_bytes)["cases"]
    requested = set(args.case)
    unknown = requested - {case["id"] for case in cases}
    if unknown:
        parser.error("Unknown case ID(s): " + ", ".join(sorted(unknown)))
    selected = [case for case in cases if not requested or case["id"] in requested]
    out.mkdir(parents=True)
    (out / "spec.json").write_bytes(spec_bytes)
    metadata = {"binary_sha256": hashlib.sha256(binary.read_bytes()).hexdigest(),
                "spec_sha256": hashlib.sha256(spec_bytes).hexdigest(), "provider": args.provider,
                "model": args.model, "without_observer": args.without_observer,
                "selected_case_ids": [case["id"] for case in selected]}
    (out / "metadata.json").write_text(json.dumps(metadata, indent=2))
    env = dict(os.environ)
    env.pop("CARGO_TARGET_DIR", None)
    env.update(PATH=str(binary.parent) + os.pathsep + env.get("PATH", ""), CARGO_INCREMENTAL="0",
               CARGO_PROFILE_DEV_DEBUG="0", CARGO_PROFILE_TEST_DEBUG="0", PYTHONNOUSERSITE="1")
    base = {"openai": "https://api.openai.com/v1", "mistral": "https://api.mistral.ai/v1",
            "gemini": "https://generativelanguage.googleapis.com/v1beta/openai"}[args.provider]
    key_name = {"openai": "OPENAI_API_KEY", "mistral": "MISTRAL_API_KEY", "gemini": "GEMINI_API_KEY"}[args.provider]
    secret = env.get(key_name)
    if not secret:
        raise SystemExit("Selected provider credential is unavailable")
    results = []
    for case in selected:
        case_out = out / case["id"]
        case_out.mkdir()
        root = case_out / "workspace"
        shutil.copytree(repo / case["tool_root"], root, ignore=shutil.ignore_patterns("target", ".git"))
        prompt = case_out / "prompt.txt"
        prompt.write_text(case["prompt"])
        baseline = None
        replay_path = root / ".spiral-coder/tui_replay.json"
        if replay_path.exists():
            baseline = json.loads(replay_path.read_text())
            # Include nonempty secondary evidence and an extension in the new
            # v1 fixture so a lossy typed round trip is observable independently.
            baseline["cases"][0]["preservation_probe"] = {"retain": True}
            baseline["cases"][0]["observer_response"]["suggestions"].append({
                "kind": "clarify", "reason": "Keep uncertainty explicit", "confidence": 0.2,
                "suggested_args": {}, "based_on": ["failure_kind"]})
            replay_path.write_text(json.dumps(baseline, indent=2))
            (case_out / "baseline.json").write_text(json.dumps(baseline, indent=2))
        case_spec = case_out / "case.json"
        case_spec.write_text(json.dumps(case, indent=2))
        command = [str(binary), "--provider", args.provider, "--base-url", base, "--model", args.model,
                   "--temperature", "0", "--max-tokens", "2048", "--timeout-seconds", "90",
                   "coupled-run", "--tool-root", str(root), "--prompt-file", str(prompt),
                   "--out-dir", str(case_out / "run"), "--max-requests", str(case["max_requests"]), "--case-spec", str(case_spec)]
        if args.without_observer:
            command.append("--without-observer")
        if case.get("session_seed"):
            seed = json.loads((root / case["session_seed"]).read_text())
            for message in seed["messages"]:
                if message.get("role") == "user" and message.get("origin") != "runtime":
                    message["content"] = case["prompt"]
                    break
            seed_path = case_out / "seed.json"
            seed_path.write_text(json.dumps(seed))
            command.extend(["--seed-session", str(seed_path)])
        started = time.monotonic()
        returncode, log, timed_out = run_bounded(command, cwd=repo, env=env, timeout=args.wall_seconds)
        (case_out / "console.log").write_text(log.replace(secret, "[REDACTED]"))
        report_path = case_out / "run/report.json"
        report = json.loads(report_path.read_text()) if report_path.exists() else {}
        execution = report.get("execution") or {}
        check_path = case_out / "run/checks.json"
        strict = json.loads(check_path.read_text()) if check_path.exists() else {}
        checks = {"all_fixed_case_checks": strict.get("ok") is True, "runtime_completed": report.get("completed") is True,
                  "within_shared_cap": len(report.get("work", {}).get("requests", [])) <= case["max_requests"],
                  "exact_checks_current": execution.get("pending_exact_checks") == [],
                  "process_ok": returncode == 0 and not timed_out}
        if baseline is not None:
            try:
                candidate = json.loads(replay_path.read_text())
                checks["preserved_existing_cases"] = preserved(baseline, candidate)
                checks["requested_new_case_exists"] = any(c.get("id") == "review-panel-replay-sensitive" for c in candidate["cases"])
                code, log, expired = run_bounded([str(binary), "tui-replay", "--spec", str(replay_path), "--out-dir", str(case_out / "oracle-replay")], cwd=root, env=env, timeout=90)
                (case_out / "oracle-replay.log").write_text(log.replace(secret, "[REDACTED]"))
                checks["all_old_and_new_replay_cases_pass"] = code == 0 and not expired and checks["requested_new_case_exists"]
            except (OSError, ValueError, KeyError):
                checks["preserved_existing_cases"] = False
        if "resume-damaged" in case["id"]:
            code, log, expired = run_bounded(["cargo", "test", "--locked"], cwd=root, env=env, timeout=90)
            (case_out / "oracle-tests.log").write_text(log.replace(secret, "[REDACTED]"))
            checks["independent_tests_pass"] = code == 0 and not expired
        result = {"id": case["id"], "ok": all(checks.values()), "checks": checks,
                  "requests": len(report.get("work", {}).get("requests", [])),
                  "assists": report.get("work", {}).get("assists", []),
                  "wall_seconds": round(time.monotonic() - started, 3), "timed_out": timed_out}
        results.append(result)
        (out / "summary.json").write_text(json.dumps({**metadata, "cases": results}, indent=2))
        print(json.dumps(result, ensure_ascii=False), flush=True)


if __name__ == "__main__":
    main()
