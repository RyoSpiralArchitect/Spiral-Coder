"""Replay the sealed catalog patches on the original tree; optionally audit the final guide."""

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


def verify(origin, final, out, target, guide=None, review_layout=False):
    if review_layout and guide is None:
        raise ValueError("The placement correction needs the sealed final guide")
    stages = [(origin, ORIGIN), (final, RECORD / "checkpoint")]
    if guide is not None:
        choices = [RECORD / "guide", RECORD / "guide/intent"]
        known = [
            folder
            for folder in choices
            if (folder / "plan.json").is_file()
            and read(folder / "plan.json")["declaration_sha256"]
            == digest(guide / "predeclared.json")
        ]
        if len(known) != 1:
            raise ValueError("Unrecognized guide episode declaration")
        guide_record = known[0]
        stages.append((guide, guide_record))
    stage_roots = [raw for raw, _ in stages]

    if any(
        path.is_relative_to(protected)
        for path in (out, target)
        for protected in [REPO, *stage_roots]
    ):
        raise ValueError(
            "Use output and Cargo cache directories outside the checkout and sealed episodes"
        )
    # Verify frozen code before importing it. The candidate never supplies the evaluator.
    original_plan, finish_plan = (
        read(folder / "plan.json") for folder in (ORIGIN, RECORD / "checkpoint")
    )
    for raw, record in stages:
        plan = read(record / "plan.json")
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

    for raw, record in stages:
        plan = read(record / "plan.json")
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
    guide_audit = None
    layout_correction = None
    with tempfile.TemporaryDirectory() as tmp:
        root = Path(tmp).resolve() / "source"
        git_snapshot(REPO, original_plan["baseline_commit"], root)
        before = inventory(root)

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
        original_guide = (root / "docs/evaluation.md").read_text()
        for episode, _ in stages:
            patch = episode / "episode/candidate.patch"
            if patch.stat().st_size:
                git("apply", "--binary", "--whitespace=nowarn", str(patch))
            expected = read(episode / "episode/candidate-inventory.json")
            for overlay in read(episode / "predeclared.json")["visible_fixture_paths"]:
                assert overlay not in before and overlay in expected
                expected.pop(overlay)
            actual = inventory(root)
            if actual != expected:
                raise ValueError(
                    f"composed patch differs from sealed candidate {episode.name}"
                )
        if review_layout:
            import difflib

            sys.path.insert(0, str(HERE))
            from guide_layout import place_at_end

            guide_path = root / "docs/evaluation.md"
            authored_bytes = guide_path.read_bytes()
            reviewed = place_at_end(original_guide, authored_bytes.decode()).encode()
            guide_path.write_bytes(reviewed)
            layout_patch = out / "review-layout.patch"
            layout_patch.write_bytes(
                b"".join(
                    difflib.diff_bytes(
                        difflib.unified_diff,
                        authored_bytes.splitlines(keepends=True),
                        reviewed.splitlines(keepends=True),
                        fromfile=b"a/docs/evaluation.md",
                        tofile=b"b/docs/evaluation.md",
                    )
                )
            )
            layout_correction = {
                "patch_sha256": digest(layout_patch),
                "model_guide_sha256": hashlib.sha256(authored_bytes).hexdigest(),
                "reviewed_guide_sha256": digest(guide_path),
                "original_guide_bytes_preserved_as_prefix": reviewed.startswith(
                    original_guide.encode()
                ),
            }
            changed_review = inventory(root)
            assert [k for k in actual if changed_review[k] != actual[k]] == [
                "docs/evaluation.md"
            ]
            assert len(changed_review) == len(actual)
            expected = actual = changed_review
        touched = sorted(
            p for p in before.keys() | actual.keys() if before.get(p) != actual.get(p)
        )
        assert set(touched) <= set(spec["editable"])
        git("add", "-N", ".")
        (out / "combined.patch").write_bytes(git("diff", "--binary", "HEAD", "--"))
        # Verify the portable cumulative patch itself, including new files and modes.
        clean = Path(tmp).resolve() / "replay"
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
            if guide is not None and all(r["passed"] for r in results):
                sys.path.insert(0, str(guide / "inputs"))
                from guide_visible import measured, discrepancies

                candidate_binary = target / "debug/spiral-coder"
                assert digest(candidate_binary) == results[0]["candidate_binary_sha256"]
                observed = measured(candidate_binary, clean)
                document = (clean / "docs/evaluation.md").read_text()
                import re

                preserved = re.sub(
                    r"(?ms)^## Read-only replay catalog[ \t]*\n.*?(?=^## |\Z)",
                    "",
                    document,
                    count=1,
                )
                original_lines = [
                    line for line in original_guide.splitlines() if line.strip()
                ]
                remaining_lines = [
                    line for line in preserved.splitlines() if line.strip()
                ]
                guide_audit = {
                    "packaged_examples": observed,
                    "discrepancies": discrepancies(document, observed),
                    "original_nonblank_lines_preserved": original_lines
                    == remaining_lines,
                }
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
    if layout_correction is not None:
        summary["maintainer_layout_correction"] = layout_correction
        summary["identical_to_unedited_model_candidate"] = False
        summary["prior_model_inventories_matched_before_correction"] = True
    if guide is not None:
        latest = read(guide_record / "results.json")
        summary.update(
            guide_receipt=latest["run_receipt_sha256"],
            guide_qualified=latest["qualified_candidate"],
            guide_audit=guide_audit,
        )
    write_json(out / "verification.json", summary)
    return summary


if __name__ == "__main__":
    parser = argparse.ArgumentParser(description=__doc__)
    for arg in ("origin", "final", "out", "target"):
        parser.add_argument(arg, type=Path)
    parser.add_argument("--guide-episode", type=Path)
    parser.add_argument(
        "--review-layout",
        action="store_true",
        help="separately apply the bounded maintainer placement correction",
    )
    args = parser.parse_args()
    print(
        json.dumps(
            verify(
                *(
                    getattr(args, k).resolve()
                    for k in ("origin", "final", "out", "target")
                ),
                guide=args.guide_episode.resolve() if args.guide_episode else None,
                review_layout=args.review_layout,
            ),
            indent=2,
        )
    )
