"""Expand the compact operator reference against the pinned Git tree, outside the candidate."""
import argparse
import hashlib
import json
from pathlib import Path
import shutil
import subprocess
import sys
import tempfile

HERE = Path(__file__).resolve().parent
REPO = HERE.parents[2]
BASE = "55351155d73675605e478e0518cc09279ae02b80"
sys.path.insert(0, str(REPO / "scripts"))
from coupled_bench.common import inventory, read_json
from self_dogfood.contracts import git_snapshot


def materialize(out, *, order_guidance=False, reuse_guidance=False, route_guidance=False, r11_handoff=False):
    if out.exists() or out.is_relative_to(REPO):
        raise ValueError("Use a fresh input directory outside the repository")
    spec_path = HERE / "luna-luna.json"
    spec = read_json(spec_path)
    with tempfile.TemporaryDirectory() as temporary:
        root = Path(temporary) / "source"
        git_snapshot(REPO, BASE, root)
        before = inventory(root)
        subprocess.run(["git", "apply", "--whitespace=error", str(HERE / "reference.patch")],
                       cwd=root, check=True)
        after = inventory(root)
        touched = {p for p in before.keys() | after.keys() if before.get(p) != after.get(p)}
        if touched != {item["target"] for item in spec["reference"]}:
            raise ValueError("Reference patch differs from the declared reference paths")
        out.mkdir(parents=True)
        for name in [spec_path.name, spec["prompt"], *(r["source"] for k in ("visible_fixtures", "probes") for r in spec[k])]:
            shutil.copy2(HERE / name, out / name)
        if order_guidance or reuse_guidance or route_guidance or r11_handoff:
            prompt = out / spec["prompt"]
            intro, separator, rest = prompt.read_bytes().partition(b"\n\n")
            if not separator:
                raise ValueError("Expected the original task introduction as the first paragraph")
            guidance = [(HERE / name).read_bytes().rstrip(b"\n") for name, enabled in
                        (("order-guidance.txt", order_guidance), ("reuse-guidance.txt", reuse_guidance),
                         ("route-guidance.txt", route_guidance), ("r11-handoff.txt", r11_handoff)) if enabled]
            prompt.write_bytes(separator.join([intro, *guidance, rest]))
        if r11_handoff:
            archive = REPO / "docs/evals/2026-11-03-diagnostic-read"
            patch = (archive / "candidate.patch").read_bytes()
            digest = hashlib.sha256(patch).hexdigest()
            if digest != read_json(archive / "plain-replay.json")["patch_sha256"]:
                raise ValueError("R11 patch differs from its independent replay receipt")
            (out / "prior_catalog_attempt.patch").write_bytes(patch)
            spec["visible_fixtures"].append({
                "source": "prior_catalog_attempt.patch", "target": "scripts/prior_catalog_attempt.patch"
            })
            (out / spec_path.name).write_text(json.dumps(spec, indent=2) + "\n")
        for item in spec["reference"]:
            dst = out / item["source"]
            dst.parent.mkdir(parents=True, exist_ok=True)
            shutil.copy2(root / item["target"], dst)


if __name__ == "__main__":
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("out", type=Path)
    parser.add_argument("--order-guidance", action="store_true",
                        help="Include the second episode's shared edit order in the task prompt")
    parser.add_argument("--reuse-guidance", action="store_true",
                        help="Encourage both roles to consider existing helpers when they fit")
    parser.add_argument("--route-guidance", action="store_true",
                        help="Describe the existing CLI entry points and compile the new route together")
    parser.add_argument("--r11-handoff", action="store_true",
                        help="Offer the prior pair-authored R11 patch as a protected readable fixture")
    args = parser.parse_args()
    materialize(args.out.resolve(), order_guidance=args.order_guidance, reuse_guidance=args.reuse_guidance,
                route_guidance=args.route_guidance, r11_handoff=args.r11_handoff)
