"""Freeze a documentation-only repair on the exact combined R11 and C1 source."""

import argparse
import json
from pathlib import Path
import shutil
import subprocess
import sys

HERE = Path(__file__).resolve().parent
REPO = HERE.parents[3]
sys.path.insert(0, str(HERE.parent))
from materialize import BASE, materialize
from checkpoint.prepare_inputs import checkpoint_commit, read_json, sha, R11_PATCH_SHA

ARCHIVE = REPO / "docs/evals/2026-11-03-catalog-route/checkpoint"
GUIDE = "docs/evaluation.md"
VISIBLE = "python3 -S -m unittest -v scripts.catalog_guide_visible"


def inputs(out):
    original = REPO / "docs/evals/2026-11-03-diagnostic-read/candidate.patch"
    current = ARCHIVE / "candidate.patch"
    if (
        sha(original) != R11_PATCH_SHA
        or sha(current) != read_json(ARCHIVE / "plain-replay.json")["patch_sha256"]
    ):
        raise ValueError("Unexpected predecessor patch")
    first = checkpoint_commit(original)
    assert first == read_json(ARCHIVE / "plan.json")["baseline_commit"]
    base = checkpoint_commit(current, first, "Exact C1 catalog delivery checkpoint")
    materialize(out)
    spec_file = out / "luna-luna.json"
    spec = read_json(spec_file)
    shutil.copyfile(HERE / "prompt.txt", out / "prompt.txt")
    shutil.copyfile(HERE / "guide_visible.py", out / "guide_visible.py")
    shutil.copyfile(
        HERE.parent / "checkpoint/delivery_visible.py", out / "delivery_visible.py"
    )
    spec["id"] = "catalog-guide-accuracy"
    spec["editable"] = spec["required_changed_paths"] = [GUIDE]
    spec["max_requests"] = 24
    spec["observer_reserved_requests"] = 6
    spec["transport_repair_requests"] = 2
    spec["reproducer_command"] = VISIBLE
    spec["visible_check_id"] = "catalog-guide-visible"
    spec["native_checks"] = [
        c["command"]
        for c in json.loads(
            (out / "prompt.txt")
            .read_text()
            .split("<task_requirements>")[1]
            .split("</task_requirements>")[0]
        )["requirements"][0]["checks"]
    ]
    spec["visible_fixtures"] += [
        {"source": file, "target": target}
        for file, target in [
            ("guide_visible.py", "scripts/catalog_guide_visible.py"),
            ("delivery_visible.py", "scripts/catalog_delivery_visible.py"),
        ]
    ]
    spec["reference"] = [x for x in spec["reference"] if x["target"] == GUIDE]
    for check in spec["checks"]:
        check["baseline"] = "pass"
        check.pop("expected_baseline_failures", None)
        check.pop("baseline_failure_contains", None)
    spec["checks"].insert(
        1,
        {
            "id": spec["visible_check_id"],
            "kind": "improvement",
            "baseline": "fail",
            "argv": VISIBLE.split(),
            "timeout_seconds": 150,
            "expected_test_count": 1,
            "expected_baseline_failures": 1,
            "baseline_failure_contains": ["catalog guide mismatch"],
        },
    )
    spec_file.write_text(json.dumps(spec, indent=2) + "\n")

    # Reference uses the stock fixture's order and tags. Preflight compares it
    # with the packaged CLI before any model receives the task.
    def original_bytes(path):
        return subprocess.check_output(["git", "show", f"{BASE}:{path}"], cwd=REPO)

    cases = json.loads(original_bytes(".spiral-coder/tui_replay.json"))["cases"]
    selected = [
        {"id": c["id"], "tags": c.get("tags", [])}
        for c in cases
        if "prefs" in c["id"].lower()
        or any("prefs" in t.lower() for t in c.get("tags", []))
    ][:2]
    rendered = json.dumps(
        {"version": 1, "total": len(selected), "cases": selected}, separators=(",", ":")
    )
    chapter = (
        "\n## Read-only replay catalog\n\nListing prints metadata without executing replay. "
        "The ASCII case-insensitive filter matches IDs or tags. Spec and tag order are retained. "
        "The cap applies after filtering, and a zero cap selects one.\n\n"
        "```console\n$ spiral tui-replay-list --filter prefs --max-cases 2\n"
        + rendered
        + "\n```\n\n"
        "An empty selection writes a diagnostic to stderr and no JSON to stdout.\n\n"
        "```console\n$ spiral tui-replay-list --filter __spiral_no_match__\n$ echo $?\n1\n```\n"
    )
    (out / spec["reference"][0]["source"]).write_bytes(
        original_bytes(GUIDE) + chapter.encode()
    )
    return base


if __name__ == "__main__":
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("out", type=Path)
    print(inputs(parser.parse_args().out.resolve()))
