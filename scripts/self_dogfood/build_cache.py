"""Invalidate this repository's artifacts when a shared cache changes workspace."""
from coupled_bench.common import sha, write_json
from .contracts import execution_env
from .process import run_process


def reset_package_cache(out, workspace, target_dir, receipt, timeout=60):
    """Call under the external cache lock, before checks or provider execution.

    Cargo can reuse root-package artifacts from another copy when the new copy's
    source mtimes are older. Keep dependency artifacts, but require a fresh build
    of spiral-coder for each baseline, reference, live and assessment workspace.
    """
    argv = ["cargo", "clean", "--locked", "--offline", "--package", "spiral-coder"]
    log = receipt.with_suffix(".log")
    result = run_process(argv, workspace, execution_env(out, target_dir), timeout, log)
    passed = result["exit_code"] == 0 and not result["timed_out"] and not result["output_limit"]
    write_json(receipt, {"version": 1, "argv": argv, "workspace": str(workspace),
                         "target_dir": str(target_dir), **result, "passed": passed,
                         "log": str(log.relative_to(out)), "log_sha256": sha(log)})
    if not passed:
        raise ValueError("Package cache reset failed; no checks or provider execution started")
    return sha(receipt)
