#!/usr/bin/env python3
"""Replay frozen status checks and posthoc boundaries on a disposable reference.

This makes no provider requests and does not replay the missing S16-S19
candidate bytes. All builds, fault switches, logs and repair exemplars live in
the new external --out directory. Historical files and test IDs stay unchanged.
"""
import argparse
import difflib
import hashlib
import importlib.util
import json
import os
from pathlib import Path
import platform
import signal
import subprocess
import sys
import time

sys.dont_write_bytecode = True
HERE = Path(__file__).resolve().parent
REPO = HERE.parents[2]
BENCH = REPO / "benchmarks/self-dogfood-v2/coupled-status"
BASELINE = "7ad8be4dca4bea603d717a3801837cb689f560ab"
FAULT_ENV = "SPIRAL_STATUS_BOUNDARY_FAULT"
FAULTS = {
    "first_parent": ("test_first_and_later_parent_linkage", 1),
    "ledger_task_id": ("test_ledger_identity_with_unchanged_counts", 1),
    "completed_active": ("test_completion_relations", 1),
    "dangling_pending": ("test_pending_absence_and_dangling_components", 6),
    "duplicate_request": ("test_duplicate_id_preserves_counts_and_charges", 1),
}
# Only setup is hoisted for status_paths: its unmodified setup builds the same
# binary on every process. Test methods and fixtures remain the frozen bytes.
UNITTEST_RUNNER = r'''
import importlib.util, json, pathlib, sys, unittest
sys.dont_write_bytecode = True
kind, source, binary, destination = sys.argv[1:]
sys.path.insert(0, str(pathlib.Path.cwd()))
sys.argv = [source, binary] if kind == "oracle" else [source]
name = {"oracle": "status_oracle", "paths": "scripts.status_paths", "boundaries": "status_boundary_diagnostics"}[kind]
spec = importlib.util.spec_from_file_location(name, source)
module = importlib.util.module_from_spec(spec); spec.loader.exec_module(module)
cls = getattr(module, {"oracle": "StatusOracle", "paths": "StatusPaths", "boundaries": "BoundaryDiagnostics"}[kind])
if kind == "paths":
    cls.setUpClass = classmethod(lambda cls: setattr(cls, "binary", pathlib.Path(binary)))
elif kind == "boundaries":
    cls.binary = pathlib.Path(binary)
suite = unittest.defaultTestLoader.loadTestsFromTestCase(cls)
def canonical(test):
    return ".".join(test.id().split(" (", 1)[0].split(".")[-2:])
identities = [canonical(test) for test in suite]
result = unittest.TextTestRunner(verbosity=2).run(suite)
payload = {"test_ids": identities, "tests_run": result.testsRun,
           "failures": [{"id": canonical(test), "label": test.id()} for test, _ in result.failures],
           "errors": [{"id": canonical(test), "label": test.id()} for test, _ in result.errors],
           "skips": [{"id": canonical(test), "reason": reason} for test, reason in result.skipped],
           "expected_failures": len(result.expectedFailures), "unexpected_successes": len(result.unexpectedSuccesses),
           "successful": result.wasSuccessful()}
pathlib.Path(destination).write_text(json.dumps(payload, indent=2) + "\n")
raise SystemExit(0 if result.wasSuccessful() else 1)
'''


def digest(path):
    return hashlib.sha256(path.read_bytes()).hexdigest()


def replace_once(text, before, after):
    if text.count(before) != 1:
        raise ValueError("Fault anchor changed: " + before)
    return text.replace(before, after, 1)


def mutate(text, fault, conditional):
    enabled = f'boundary_fault("{fault}")' if conditional else "true"
    if fault == "first_parent":
        return replace_once(text, "r.parent_task_id.as_deref() == parent &&",
                            f"(({enabled} && index == 0) || r.parent_task_id.as_deref() == parent) &&")
    if fault == "ledger_task_id":
        return replace_once(text, "task == r.task_id &&", f"({enabled} || task == r.task_id) &&")
    if fault == "completed_active":
        # Both guards reject completed+active independently; this is explicitly
        # a compound fault, not evidence that one deletion recreates S19.
        text = replace_once(text, 'ensure!(!m.completed, "completed run has pending iteration");',
                            f'ensure!({enabled} || !m.completed, "completed run has pending iteration");')
        return replace_once(text, 'ensure!(m.stop_reason.is_none(), "active run has stop reason");',
                            f'ensure!({enabled} || m.stop_reason.is_none(), "active run has stop reason");')
    if fault == "dangling_pending":
        return replace_once(text, "    let mut path = root.to_path_buf();",
                            f"    if {enabled} && optional && !root.join(relative).exists() {{ return Ok(None); }}\n"
                            "    let mut path = root.to_path_buf();")
    if fault == "duplicate_request":
        return replace_once(text, "ids.insert(r.id)", f"({enabled} || ids.insert(r.id))")
    raise ValueError("Unknown fault: " + fault)


def clean_env(target):
    env = {k: v for k, v in os.environ.items()
           if not any(part in k.upper() for part in ("API_KEY", "TOKEN", "SECRET", "CREDENTIAL", "PASSWORD"))}
    env.pop(FAULT_ENV, None)
    env.pop("SPIRAL_STATUS_AUDIT_FAULT", None)
    env.update(PYTHONDONTWRITEBYTECODE="1", SPIRAL_CODER_NO_DOTENV="1", CARGO_TARGET_DIR=str(target))
    return env


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--out", type=Path, required=True)
    parser.add_argument("--target-dir", type=Path)
    args = parser.parse_args()
    out = args.out.resolve()
    target = (args.target_dir or out / "target").resolve()
    if out.exists() or out.is_relative_to(REPO) or target.is_relative_to(REPO):
        parser.error("--out must be a NEW external directory; --target-dir must also be outside the repository")
    provenance = json.loads((HERE / "provenance.json").read_text())
    expected = {REPO / path: item["sha256"]
                for path, item in provenance["original_file_inventory"]["files"].items()}
    extra = [HERE / "provenance.json", HERE / "test_boundaries.py", Path(__file__),
             REPO / "scripts/self_dogfood/contracts.py", REPO / "src/config.rs",
             REPO / "src/config/env_test_support.rs"]
    expected.update({path: digest(path) for path in extra})

    def verify_inputs():
        for path, sha in expected.items():
            if not path.is_file() or path.is_symlink() or digest(path) != sha:
                raise ValueError("Audit input changed: " + str(path.relative_to(REPO)))

    verify_inputs()
    out.mkdir(parents=True, exist_ok=False)
    logs = out / "logs"; logs.mkdir()
    env = clean_env(target)
    result = dict(version=1, model_calls=0, provider_calls=0, archived_candidate_replay=False,
                  fresh_qualification=False, historical_outcomes_rerated=False, passed=False,
                  baseline_commit=BASELINE, pinned_pr_head=provenance["refs"]["pr_head_commit"],
                  environment={"platform": platform.platform(), "python": sys.version,
                               "credential_variables_removed": True, "fault_unset_for_reference": True,
                               "target_dir": str(target), "bytecode_writes_disabled": True},
                  input_sha256={str(p.relative_to(REPO)): sha for p, sha in expected.items()},
                  setup_adaptation="Frozen status_paths setUpClass build is hoisted into one locked build; original test methods are unchanged.",
                  repair_exemplar_scope="Posthoc standalone mutants -> frozen reference, not patches of original S16-S19 candidates.",
                  standalone_repair_exemplars_compiled=False, executions=[], repair_exemplars={})

    def run(label, argv, cwd, fault=None, timeout=180):
        child_env = dict(env)
        if fault is not None:
            child_env[FAULT_ENV] = fault
        stdout = logs / f"{label}.stdout.log"; stderr = logs / f"{label}.stderr.log"
        started = time.monotonic(); timed_out = False
        with stdout.open("wb") as output, stderr.open("wb") as error:
            process = subprocess.Popen([str(x) for x in argv], cwd=cwd, env=child_env,
                                       stdout=output, stderr=error, start_new_session=True)
            try:
                code = process.wait(timeout=timeout)
            except subprocess.TimeoutExpired:
                timed_out = True
                os.killpg(process.pid, signal.SIGKILL)
                code = process.wait()
        row = dict(label=label, argv=[str(x) for x in argv], cwd=str(cwd), fault=fault,
                   exit_code=code, timed_out=timed_out, wall_seconds=round(time.monotonic()-started, 3),
                   stdout={"path": str(stdout.relative_to(out)), "sha256": digest(stdout)},
                   stderr={"path": str(stderr.relative_to(out)), "sha256": digest(stderr)})
        result["executions"].append(row)
        return row

    try:
        for name, command in (("cargo", ["cargo", "--version"]), ("rustc", ["rustc", "--version"]),
                              ("git-head", ["git", "rev-parse", "HEAD"])):
            row = run(name, command, REPO, timeout=30)
            if row["exit_code"] != 0 or row["timed_out"]:
                raise RuntimeError("Toolchain observation failed: " + name)
            result["environment"][name] = (out / row["stdout"]["path"]).read_text().strip()
        sys.path.insert(0, str(REPO / "scripts"))
        from self_dogfood.contracts import apply_overlays, git_snapshot
        spec = importlib.util.spec_from_file_location("frozen_materialize", BENCH / "materialize.py")
        module = importlib.util.module_from_spec(spec); spec.loader.exec_module(module)
        # materialize invokes only git_snapshot and cargo fmt. Its child processes
        # inherit the same stripped environment, with no model request entry point.
        previous_env = dict(os.environ)
        try:
            os.environ.clear(); os.environ.update(env)
            module.materialize(out / "inputs", BASELINE)
            workspace = out / "workspace"
            git_snapshot(REPO, BASELINE, workspace)
        finally:
            os.environ.clear(); os.environ.update(previous_env)
        declaration = json.loads((out / "inputs/luna-luna.json").read_text())
        for key in ("visible_fixtures", "probes", "reference"):
            apply_overlays(workspace, out / "inputs", declaration[key])
        raw = (BENCH / "reference.rs").read_text()
        instrumented = raw
        repairs = out / "repair-exemplars"; repairs.mkdir()
        for fault in FAULTS:
            instrumented = mutate(instrumented, fault, True)
            mutant = mutate(raw, fault, False)
            source = repairs / f"{fault}.mutant.rs"; source.write_text(mutant)
            patch = repairs / f"{fault}.repair.patch"
            patch.write_text("".join(difflib.unified_diff(mutant.splitlines(True), raw.splitlines(True),
                                                       fromfile="a/src/coupled_status.rs", tofile="b/src/coupled_status.rs")))
            result["repair_exemplars"][fault] = {"source": str(source.relative_to(out)), "source_sha256": digest(source),
                                                "patch": str(patch.relative_to(out)), "patch_sha256": digest(patch),
                                                "compound_fault": fault == "completed_active"}
        instrumented += f'\nfn boundary_fault(name: &str) -> bool {{ std::env::var("{FAULT_ENV}").as_deref() == Ok(name) }}\n'
        implementation = workspace / "src/coupled_status.rs"; implementation.write_text(instrumented)
        result.update(raw_reference_sha256=digest(BENCH / "reference.rs"), instrumented_sha256=digest(implementation))
        built = run("build", ["cargo", "build", "--locked", "--bin", "spiral-coder"], workspace, timeout=900)
        if built["exit_code"] != 0 or built["timed_out"]:
            raise RuntimeError("Reference build failed; compile errors/timeouts are not fault detection")
        binary = target / "debug/spiral-coder"
        result["binary_sha256"] = digest(binary)
        wrapper = out / "run_unittest.py"; wrapper.write_text(UNITTEST_RUNNER)

        def suite(kind, fault=None, failed_method=None, failures=0):
            label = f"{kind}-{fault or 'reference'}"
            source = {"oracle": BENCH / "status_oracle.py", "paths": BENCH / "status_paths.py",
                      "boundaries": HERE / "test_boundaries.py"}[kind]
            summary = out / f"{label}.json"
            row = run(label, [sys.executable, "-B", "-S", wrapper, kind, source, binary, summary], workspace, fault)
            if row["timed_out"] or not summary.is_file():
                raise RuntimeError("Suite did not complete: " + label)
            data = json.loads(summary.read_text()); row["tests"] = data
            wanted = (provenance["original_contract"]["oracle_ids"] if kind == "oracle" else
                      provenance["original_contract"]["path_suite_ids"] if kind == "paths" else
                      ["BoundaryDiagnostics." + method for method, _ in FAULTS.values()])
            expected_count = {"oracle": 24, "paths": 3, "boundaries": 5}[kind]
            observed_methods = {item["id"].split(".")[-1] for item in data["failures"]}
            valid = (row["exit_code"] == (1 if failures else 0) and data["tests_run"] == expected_count
                     and sorted(data["test_ids"]) == sorted(wanted) and not data["errors"] and not data["skips"]
                     and data["expected_failures"] == data["unexpected_successes"] == 0
                     and len(data["failures"]) == failures
                     and observed_methods == ({failed_method} if failures else set())
                     and data["successful"] == (failures == 0))
            row["expected_outcome_observed"] = valid
            if not valid:
                raise RuntimeError("Unexpected assertion/error/test identity outcome: " + label)

        suite("oracle")
        for fault, method in (("first_parent", "test_wrong_parent"), ("ledger_task_id", "test_wrong_task_id"),
                              ("completed_active", "test_completion_conflict")):
            suite("oracle", fault, method, 1)
        suite("paths")
        suite("paths", "dangling_pending", "test_symlink_components_even_dangling_are_rejected", 3)
        suite("boundaries")
        for fault, (method, count) in FAULTS.items():
            suite("boundaries", fault, method, count)
        result["passed"] = True
    except Exception as error:
        result["error"] = f"{type(error).__name__}: {error}"
    finally:
        try:
            verify_inputs(); result["original_and_current_inputs_unchanged"] = True
        except Exception as error:
            result.update(passed=False, original_and_current_inputs_unchanged=False, preservation_error=str(error))
        (out / "result.json").write_text(json.dumps(result, indent=2) + "\n")
    print(json.dumps({"passed": result["passed"], "result": str(out / "result.json")}))
    return 0 if result["passed"] else 1


if __name__ == "__main__":
    raise SystemExit(main())
