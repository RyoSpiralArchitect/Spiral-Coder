"""Replay an operator's dependency order after r1, independent of the live candidate."""
import argparse
from pathlib import Path
import shutil
import sys
import tempfile

PIN = "41388f94fa8a431a8c81a65f358f40f0ef6e525481b36e8b294783a130bc8e41"
STAGES = [
    ("add-leaf", ["src/tui_replay/catalog.rs"]),
    ("register-module-and-share-selection", ["src/tui_replay.rs"]),
    ("dispatch-cli", ["src/main.rs"]),
    ("expose-spiral-and-document", ["npm/spiral-coder/lib/launcher.cjs", "docs/evaluation.md"]),
]


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("episode", type=Path)
    parser.add_argument("output", type=Path)
    args = parser.parse_args()
    raw, output = args.episode.resolve(), args.output.resolve()
    if output.is_relative_to(raw) or output.exists():
        parser.error("Use a new output directory outside the episode")
    sys.path.insert(0, str(raw / "driver"))
    from coupled_bench.common import read_json, sha, write_json
    from self_dogfood.build_cache import reset_package_cache
    from self_dogfood.contracts import apply_overlays, execution_env, load_frozen
    from self_dogfood.process import run_process
    from self_dogfood.runner import cache_lock

    _, spec = load_frozen(raw, PIN)
    by_target = {o["target"]: o for o in spec["reference"]}
    assert set(by_target) == {p for _, paths in STAGES for p in paths}
    assert not output.is_relative_to(Path(read_json(raw / "layout.json")["source_repository"]))
    target = Path(read_json(raw / "layout.json")["target_dir"])
    binary = target / "debug/spiral-coder"
    output.mkdir(parents=True)
    rows = []
    with tempfile.TemporaryDirectory(dir=output) as tmp, cache_lock(target):
        workspace = Path(tmp) / "workspace"
        shutil.copytree(raw / "baseline", workspace)
        reset_package_cache(output, workspace, target, output / "package-cache-reset.json")
        for stage, paths in STAGES:
            apply_overlays(workspace, raw / "inputs", [by_target[p] for p in paths])
            commands = [["cargo", "build", "--locked", "--bin", "spiral-coder"]]
            if stage == "register-module-and-share-selection":
                commands.append(["python3", "-S", str(raw / "inputs/regular_replay.py"), str(binary), str(output)])
            if stage == "dispatch-cli":
                commands.append(["python3", "-S", "-m", "unittest", "-v", "scripts.catalog_visible"])
            if stage == "expose-spiral-and-document":
                commands.append(["python3", "-S", str(raw / "inputs/packaged_spiral.py"), str(binary)])
            for i, argv in enumerate(commands):
                log = output / f"{stage}-{i}.log"
                result = run_process(argv, workspace, execution_env(output, target), 180, log)
                rows.append({"stage": stage, "added_reference_paths": paths, "argv": argv, **result,
                             "log_sha256": sha(log), "binary_sha256": sha(binary) if binary.exists() else None})
                if result["exit_code"] != 0 or result["timed_out"] or result["output_limit"]:
                    break
            if rows[-1]["exit_code"] != 0 or rows[-1]["timed_out"] or rows[-1]["output_limit"]:
                break
    load_frozen(raw, PIN)
    passed = len(rows) == 7 and all(r["exit_code"] == 0 and not r["output_limit"] and not r["timed_out"] for r in rows)
    write_json(output / "results.json", {"type": "post-run operator reference sequencing",
               "predeclaration_sha256": PIN, "provider_calls": 0, "all_passed": passed, "records": rows})
    return 0 if passed else 1


if __name__ == "__main__":
    raise SystemExit(main())
