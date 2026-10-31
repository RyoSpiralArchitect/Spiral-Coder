"""Expand the compact operator reference against the pinned Git tree, outside the candidate."""
import argparse
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


def materialize(out):
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
        for item in spec["reference"]:
            dst = out / item["source"]
            dst.parent.mkdir(parents=True, exist_ok=True)
            shutil.copy2(root / item["target"], dst)


if __name__ == "__main__":
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("out", type=Path)
    materialize(parser.parse_args().out.resolve())
