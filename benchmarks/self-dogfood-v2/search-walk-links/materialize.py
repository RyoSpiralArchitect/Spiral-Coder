"""Materialize a compact reference in a fresh directory outside the source checkout."""
import argparse
import json
from pathlib import Path
import shutil
import subprocess

HERE = Path(__file__).resolve().parent
BASE = "527413cc114e0f621ccbfeb9eba6bd25d2531c15"


def materialize(out):
    repo = HERE.parents[2]
    if out.exists() or out.is_relative_to(repo):
        raise ValueError("choose a fresh external directory")
    source = subprocess.check_output(["git", "show", f"{BASE}:src/file_tools.rs"], cwd=repo)
    marker = b"        for entry in rd.flatten() {\n            let path = entry.path();"
    prefix, function = source.split(b"pub fn tool_search_files(", 1)
    function, tail = function.split(b"\n}\n", 1)
    if function.count(marker) != 1:
        raise ValueError("unexpected frozen source")
    # Reference is visible only to preparation and verification, never to the pair.
    reference = function.replace(marker, b"        for entry in rd.flatten() {\n            if !entry\n                .file_type()\n                .is_ok_and(|ty| ty.is_file() || ty.is_dir())\n            {\n                continue;\n            }\n            let path = entry.path();")
    spec = json.loads((HERE / "luna-luna.json").read_bytes())
    out.mkdir(parents=True)
    for name in {"luna-luna.json", spec["prompt"], *(item["source"] for key in ("probes", "visible_fixtures") for item in spec[key])}:
        shutil.copyfile(HERE/name, out/name)
    (out / "reference.rs").write_bytes(prefix + b"pub fn tool_search_files(" + reference + b"\n}\n" + tail)


if __name__ == "__main__":
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("out", type=Path)
    materialize(parser.parse_args().out.resolve())
