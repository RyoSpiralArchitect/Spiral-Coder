"""Test two unexecuted R16 proposals on disposable source with the real CLI."""

import argparse
import json
from pathlib import Path
import re
import shutil
import subprocess
import sys
import tempfile

HERE = Path(__file__).resolve().parent
REPO = HERE.parents[3]
BENCH = REPO / "benchmarks/self-dogfood-v2/replay-catalog"
sys.path.insert(0, str(REPO / "scripts"))
from coupled_bench.common import inventory, read_json, sha
from self_dogfood.contracts import execution_env, git_snapshot
from self_dogfood.runner import cache_lock


def replay(target):
    proposals = read_json(HERE / "proposals.json")
    patch = HERE / "candidate.patch"
    original = read_json(HERE / "plain-replay.json")
    assert original["matches"] and sha(patch) == original["patch_sha256"]
    assert (
        proposals["source_run_sha256"]
        == read_json(HERE / "results.json")["run_receipt_sha256"]
    )
    with tempfile.TemporaryDirectory() as name:
        temp = Path(name).resolve()
        target = target.resolve() if target else temp / "target"
        if target.is_relative_to(REPO):
            raise ValueError(
                "Choose a disposable target directory outside this checkout"
            )
        with cache_lock(target):
            root = temp / "source"
            git_snapshot(REPO, original["baseline_commit"], root)
            subprocess.run(["git", "apply", "--binary", patch], cwd=root, check=True)
            source = root / "src/main.rs"
            assert sha(source) == proposals["candidate_main_sha256"]
            visible = BENCH / "catalog_visible.py"
            assert sha(visible) == proposals["visible_fixture_sha256"]
            shutil.copy2(visible, root / "scripts/catalog_visible.py")
            env = execution_env(temp, target)

            def run(*args):
                return subprocess.run(
                    args, cwd=root, env=env, capture_output=True, text=True, timeout=180
                )

            def build():
                process = run("cargo", "build", "--locked", "--bin", "spiral-coder")
                return {
                    "exit": process.returncode,
                    "rust_error_codes": sorted(
                        set(re.findall(r"^error\[(E\d{4})\]", process.stderr, re.M))
                    ),
                }

            run(
                "cargo", "clean", "--locked", "--offline", "-p", "spiral-coder"
            ).check_returncode()
            before = build()
            contents = source.read_text()
            edit = proposals["blocked_edit"]["arguments"]
            assert edit["path"] == "src/main.rs" and contents.count(edit["search"]) == 1
            contents = contents.replace(edit["search"], edit["replace"])
            source.write_text(contents)
            corrected_alias = build()
            # Place the one recorded added line next to the sibling on the actual
            # tree. The rest of the multi-hunk proposal never applied.
            sibling = "        Some(Command::TuiReplay(args)) => tui_replay::run(args, cli.common).await,"
            arm = proposals["unapplied_dispatch"]["added_line"]
            assert contents.count(sibling) == 1 and arm not in contents
            source.write_text(contents.replace(sibling, sibling + "\n" + arm))
            wired = build()
            assert before["exit"] and "E0774" in before["rust_error_codes"], before
            assert corrected_alias["exit"] and corrected_alias["rust_error_codes"] == [
                "E0004"
            ], corrected_alias
            assert wired["exit"] == 0, wired
            snapshot = inventory(root)
            binary = target / "debug/spiral-coder"
            rows = {}
            for check, args in (
                (
                    "catalog-visible",
                    ["-m", "unittest", "-v", "scripts.catalog_visible"],
                ),
                ("held-out-catalog", [BENCH / "catalog_oracle.py", binary]),
                ("packaged-spiral", [BENCH / "packaged_spiral.py", binary]),
            ):
                process = run(sys.executable, "-S", *args)
                output = process.stderr + process.stdout
                summary = re.search(r"^Ran (\d+) tests? in ", output, re.M)
                if summary is None:
                    raise ValueError(
                        f"{check} did not report a unittest count (exit {process.returncode})"
                    )
                rows[check] = {
                    "exit": process.returncode,
                    "tests_run": int(summary[1]),
                    "failed": re.findall(r"^(?:FAIL|ERROR): (\S+)", output, re.M),
                }
            assert snapshot == inventory(root)
    return {
        "source_run_sha256": proposals["source_run_sha256"],
        "patch_sha256": sha(patch),
        "original_build": before,
        "after_recorded_alias_edit": corrected_alias,
        "after_placing_recorded_dispatch": wired,
        "checks_on_derivative": rows,
        "candidate_qualification_changed": False,
        "provider_requests": 0,
    }


if __name__ == "__main__":
    cli = argparse.ArgumentParser(description=__doc__)
    cli.add_argument("--target-dir", type=Path, help="external disposable Cargo cache")
    print(json.dumps(replay(cli.parse_args().target_dir), indent=2))
