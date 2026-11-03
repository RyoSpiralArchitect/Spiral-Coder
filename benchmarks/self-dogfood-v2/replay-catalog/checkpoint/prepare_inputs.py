"""Build a reproducible, unmodified R11 checkpoint and its independent follow-up inputs."""

import argparse
import json
import os
from pathlib import Path
import shutil
import subprocess
import sys
import tempfile

HERE = Path(__file__).resolve().parent
REPO = HERE.parents[3]
sys.path.insert(0, str(HERE.parent))
from materialize import BASE, materialize

sys.path.insert(0, str(REPO / "scripts"))
from coupled_bench.common import read_json, sha

R11_PATCH_SHA = "7ed74db18a0b5be96fdfb84c21348b22cef08b90dce51cc51d0708e7c5db5c1f"
ORIGINAL_RUST_TESTS = "03ee93445176ec84dfd9e87e64461b9151ccdf14f27e3934c488a48780240dc6"


def checkpoint_commit(
    patch, parent=BASE, title="Exact unqualified R11 catalog checkpoint"
):
    with tempfile.TemporaryDirectory() as temp:
        env = dict(os.environ, GIT_INDEX_FILE=str(Path(temp) / "index"))
        for role in ("AUTHOR", "COMMITTER"):
            env.update(
                {
                    f"GIT_{role}_NAME": "Spiral-Coder benchmark",
                    f"GIT_{role}_EMAIL": "fixture@localhost",
                    f"GIT_{role}_DATE": "2000-01-01T00:00:00+0000",
                }
            )

        def git(*args):
            return subprocess.check_output(
                ["git", "-c", "commit.gpgSign=false", *args],
                env=env,
                cwd=REPO,
                text=True,
            ).strip()

        git("read-tree", parent)
        git("apply", "--cached", "--binary", "--whitespace=nowarn", str(patch))
        return git(
            "commit-tree",
            git("write-tree"),
            "-p",
            parent,
            "-m",
            title,
        )


def inputs(out, identities):
    patch = REPO / "docs/evals/2026-11-03-diagnostic-read/candidate.patch"
    if sha(patch) != R11_PATCH_SHA or sha(identities) != ORIGINAL_RUST_TESTS:
        raise ValueError(
            "The original patch or stock Rust identities do not match their receipts"
        )
    commit = checkpoint_commit(patch)
    materialize(out)
    shutil.copyfile(HERE / "prompt.txt", out / "prompt.txt")
    shutil.copyfile(HERE / "delivery_visible.py", out / "delivery_visible.py")
    shutil.copyfile(identities, out / "origin-rust-tests.json")
    spec_path = out / "luna-luna.json"
    spec = read_json(spec_path)
    spec["id"] = "catalog-checkpoint-delivery"
    spec["reproducer_command"] = (
        "python3 -S -m unittest -v scripts.catalog_delivery_visible"
    )
    spec["native_checks"] = [
        spec["reproducer_command"],
        spec["native_checks"][0],
        "cargo fmt --all -- --check",
        spec["native_checks"][1],
    ]
    spec["visible_check_id"] = "catalog-delivery-visible"
    spec["required_changed_paths"] = [
        "src/tui_replay.rs",
        "docs/evaluation.md",
        "npm/spiral-coder/lib/launcher.cjs",
    ]
    spec["visible_fixtures"].append(
        {
            "source": "delivery_visible.py",
            "target": "scripts/catalog_delivery_visible.py",
        }
    )
    spec["probes"].append(
        {
            "source": "origin-rust-tests.json",
            "target": ".catalog-checks/origin-rust-tests.json",
        }
    )
    checks = {c["id"]: c for c in spec["checks"]}
    checks["catalog-visible"]["baseline"] = "pass"
    checks["held-out-catalog"].update(
        expected_baseline_failures=1,
        baseline_failure_contains=["FAIL: test_usage_is_documented"],
    )
    checks["stock-regressions"].update(
        baseline="fail", baseline_failure_contains=["Diff in ", "src/tui_replay.rs:"]
    )
    spec["checks"].insert(
        1,
        {
            "id": spec["visible_check_id"],
            "kind": "improvement",
            "baseline": "fail",
            "argv": spec["reproducer_command"].split(),
            "timeout_seconds": 60,
            "expected_test_count": 1,
            "expected_baseline_failures": 1,
            "baseline_failure_contains": [
                "installed alias diverted the listing to tui"
            ],
        },
    )
    # The unformatted checkpoint exits the stock suite before it can produce a
    # baseline inventory. Compare against the original, already hash-pinned list.
    stock = out / "stock_checks.py"
    source = stock.read_text()
    original = 'RECEIPTS.parents[1] / "baseline/artifacts/rust-tests.json"'
    if source.count(original) != 1:
        raise ValueError("Unexpected original stock verifier")
    stock.write_text(
        source.replace(original, 'Path(".catalog-checks/origin-rust-tests.json")')
    )
    spec_path.write_text(json.dumps(spec, indent=2) + "\n")
    return commit


if __name__ == "__main__":
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("out", type=Path)
    parser.add_argument("--origin-test-identities", type=Path, required=True)
    arguments = parser.parse_args()
    print(inputs(arguments.out.resolve(), arguments.origin_test_identities.resolve()))
