"""Replay two signed model patches on the untouched catalog baseline and run its original probes."""

import argparse
import hashlib
import json
from pathlib import Path
import shutil
import subprocess
import sys
import tempfile

HERE = Path(__file__).resolve().parent
REPO = HERE.parents[3]
RECORD = REPO / "docs/evals/2026-11-03-catalog-route"
ORIGIN = REPO / "docs/evals/2026-11-03-diagnostic-read"
STOCK_HASH = "03ee93445176ec84dfd9e87e64461b9151ccdf14f27e3934c488a48780240dc6"


def read(path):
    return json.loads(path.read_bytes())


def digest(path):
    return hashlib.sha256(path.read_bytes()).hexdigest()


def verify(origin, final, out, target):
    # Verify frozen code before importing it. The candidate never supplies the evaluator.
    original_plan, finish_plan = (
        read(folder / "plan.json") for folder in (ORIGIN, RECORD / "checkpoint")
    )
    for raw, plan in ((origin, original_plan), (final, finish_plan)):
        assert digest(raw / "predeclared.json") == plan["declaration_sha256"]
        declaration = read(raw / "predeclared.json")
        assert all(
            digest(raw / "driver" / name) == value
            for name, value in declaration["driver_sha256"].items()
        )
        assert all(
            digest(raw / "inputs" / name) == value["sha256"]
            for name, value in declaration["inputs_inventory"].items()
        )
    sys.path.insert(0, str(origin / "driver"))
    from coupled_bench.common import inventory, write_json
    from self_dogfood.assessment import checks
    from self_dogfood.contracts import git_snapshot
    from self_dogfood.evidence import bound_run
    from self_dogfood.runner import cache_lock

    for raw, plan, record in (
        (origin, original_plan, ORIGIN),
        (final, finish_plan, RECORD / "checkpoint"),
    ):
        receipt = read(record / "results.json")["run_receipt_sha256"]
        assert bound_run(raw, plan["declaration_sha256"], receipt)
    assert finish_plan["precursor_patch_sha256"] == digest(
        origin / "episode/candidate.patch"
    )
    assert finish_plan["original_baseline_commit"] == original_plan["baseline_commit"]
    assert digest(origin / "checks/baseline/artifacts/rust-tests.json") == STOCK_HASH
    if out.exists():
        raise ValueError("choose a fresh output directory")
    out.mkdir(parents=True)
    spec = read(origin / "inputs/spec.json")
    expected = read(final / "episode/candidate-inventory.json")
    with tempfile.TemporaryDirectory() as tmp:
        root = Path(tmp) / "source"
        git_snapshot(REPO, original_plan["baseline_commit"], root)
        before = inventory(root)
        for overlay in read(final / "predeclared.json")["visible_fixture_paths"]:
            assert overlay not in before and overlay in expected
            expected.pop(overlay)

        def git(*args):
            return subprocess.check_output(["git", *args], cwd=root)

        git("init", "-q")
        git("add", "-f", ".")
        git(
            "-c",
            "user.name=Replay fixture",
            "-c",
            "user.email=fixture@localhost",
            "-c",
            "commit.gpgSign=false",
            "commit",
            "-qm",
            "Source",
        )
        for episode in (origin, final):
            patch = episode / "episode/candidate.patch"
            if patch.stat().st_size:
                git("apply", "--binary", "--whitespace=nowarn", str(patch))
        actual = inventory(root)
        if actual != expected:
            raise ValueError("composed patch differs from sealed candidate")
        touched = sorted(
            p for p in before.keys() | actual.keys() if before.get(p) != actual.get(p)
        )
        assert set(touched) <= set(spec["editable"])
        git("add", "-N", ".")
        (out / "combined.patch").write_bytes(git("diff", "--binary", "HEAD", "--"))
        # Verify the portable cumulative patch itself, including new files and modes.
        clean = Path(tmp) / "replay"
        git_snapshot(REPO, original_plan["baseline_commit"], clean)
        subprocess.run(
            [
                "git",
                "apply",
                "--binary",
                "--whitespace=nowarn",
                str(out / "combined.patch"),
            ],
            cwd=clean,
            check=True,
        )
        assert inventory(clean) == expected
        shutil.copytree(origin / "baseline", out / "baseline")
        assert (
            inventory(out / "baseline")
            == read(origin / "predeclared.json")["baseline_inventory"]
        )
        shutil.copytree(origin / "inputs", out / "inputs")
        (out / "scratch").mkdir()
        artifacts = out / "checks/baseline/artifacts"
        artifacts.mkdir(parents=True)
        shutil.copyfile(
            origin / "checks/baseline/artifacts/rust-tests.json",
            artifacts / "rust-tests.json",
        )
        write_json(out / "layout.json", {"target_dir": str(target)})
        with cache_lock(target):
            results = checks(out, spec, "combined", candidate=clean)
    summary = {
        "original_baseline_commit": original_plan["baseline_commit"],
        "precursor_receipt": read(ORIGIN / "results.json")["run_receipt_sha256"],
        "continuation_receipt": read(RECORD / "checkpoint/results.json")[
            "run_receipt_sha256"
        ],
        "continuation_qualified": read(RECORD / "checkpoint/results.json")[
            "qualified_candidate"
        ],
        "patch_sha256": digest(out / "combined.patch"),
        "changed_paths": touched,
        "inventory_matches": True,
        "all_original_external_checks_passed": all(r["passed"] for r in results),
        "checks": [
            {
                k: r[k]
                for k in ("id", "passed", "exit_code", "test_summary", "log_sha256")
            }
            for r in results
        ],
        "external_provider_requests": 0,
    }
    write_json(out / "verification.json", summary)
    return summary


if __name__ == "__main__":
    parser = argparse.ArgumentParser(description=__doc__)
    for arg in ("origin", "final", "out", "target"):
        parser.add_argument(arg, type=Path)
    args = parser.parse_args()
    print(
        json.dumps(
            verify(
                *(
                    getattr(args, k).resolve()
                    for k in ("origin", "final", "out", "target")
                )
            ),
            indent=2,
        )
    )
