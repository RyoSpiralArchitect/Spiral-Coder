"""Rebuild the exact R12 patch, then remove just its CLI registration on a disposable tree."""

import argparse
import json
from pathlib import Path
import subprocess
import sys
import tempfile

HERE = Path(__file__).resolve().parent
REPO = HERE.parents[2]
sys.path.insert(0, str(REPO / "scripts"))
from coupled_bench.common import read_json, sha
from self_dogfood.contracts import execution_env, git_snapshot
from self_dogfood.runner import cache_lock


def replay(target):
    if target.is_relative_to(REPO):
        raise ValueError("Choose a disposable Cargo cache outside the checkout")
    record = HERE.parent / "2026-11-03-diagnostic-read/r12"
    patch = record / "candidate.patch"
    manifest = read_json(record / "plain-replay.json")
    assert manifest["matches"] and sha(patch) == manifest["patch_sha256"]
    with tempfile.TemporaryDirectory() as tmp, cache_lock(target):
        scratch = Path(tmp)
        root = scratch / "source"
        git_snapshot(REPO, manifest["baseline_commit"], root)
        main = root / "src/main.rs"
        original = main.read_bytes()
        subprocess.run(["git", "apply", "--binary", str(patch)], cwd=root, check=True)
        env = execution_env(scratch, target)
        subprocess.run(
            ["cargo", "clean", "--locked", "--offline", "-p", "spiral-coder"],
            cwd=root,
            env=env,
            check=True,
            capture_output=True,
        )
        build = [
            "cargo",
            "build",
            "--locked",
            "--offline",
            "--bin",
            "spiral-coder",
            "--message-format=json",
        ]
        failed = subprocess.run(
            build, cwd=root, env=env, capture_output=True, text=True, timeout=180
        )
        messages = [json.loads(line) for line in failed.stdout.splitlines()]
        diagnostics = []
        for event in messages:
            item = event.get("message") or {}
            if item.get("level") != "error" or not item.get("code"):
                continue
            locations = [
                {"file": span["file_name"], "line": span["line_start"]}
                for span in item["spans"]
                if span["is_primary"]
            ]
            diagnostics.append(
                {
                    "code": item["code"]["code"],
                    "message": item["message"],
                    "locations": locations,
                }
            )
        assert failed.returncode and {row["code"] for row in diagnostics} == {
            "E0425",
            "E0004",
        }, diagnostics
        assert all(
            any(x["file"] == "src/main.rs" for x in row["locations"])
            for row in diagnostics
        )
        main.write_bytes(original)
        subprocess.run(
            build, cwd=root, env=env, check=True, capture_output=True, timeout=180
        )
        binary = target / "debug/spiral-coder"
        help_text = subprocess.check_output(
            [binary, "--help"], cwd=root, env=env, text=True
        )
        assert "tui-replay-list" not in help_text and "tui-replay" in help_text
    return {
        "source_run_receipt_sha256": read_json(record / "results.json")[
            "run_receipt_sha256"
        ],
        "baseline_commit": manifest["baseline_commit"],
        "patch_sha256": sha(patch),
        "candidate_build_exit": failed.returncode,
        "native_compiler_diagnostics": diagnostics,
        "control": {
            "reverted_only": "src/main.rs",
            "build_exit": 0,
            "catalog_in_help": False,
            "ordinary_replay_in_help": True,
        },
        "provider_requests": 0,
    }


if __name__ == "__main__":
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument(
        "--target-dir",
        required=True,
        type=Path,
        help="external disposable Cargo build cache",
    )
    print(json.dumps(replay(parser.parse_args().target_dir.resolve()), indent=2))
