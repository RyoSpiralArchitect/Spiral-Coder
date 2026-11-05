"""Exercise the same helper lookup on both native executors and frozen source bytes."""
import argparse
import hashlib
import json
from pathlib import Path
import sys
import tempfile

REPO = Path(__file__).resolve().parents[3]
sys.path.insert(0, str(REPO/"benchmarks/self-dogfood-v2/search-walk-links"))
from fixture import native_search


def digest(data):
    return hashlib.sha256(data).hexdigest()


def run(binary, episode):
    file = "src/tui_replay.rs"
    data = (episode/"baseline"/file).read_bytes()
    pin = json.loads((REPO/"docs/evals/2026-11-03-scoped-lookup/plan.json").read_bytes())
    declaration = (episode/"predeclared.json").read_bytes()
    assert digest(declaration) == pin["declaration_sha256"]
    assert digest(data) == json.loads(declaration)["baseline_inventory"][file]["sha256"]
    with tempfile.TemporaryDirectory() as tmp:
        root=Path(tmp)/"workspace"; (root/"src").mkdir(parents=True)
        (root/file).write_bytes(data)
        with native_search(binary,root) as search:
            rows={}
            for name in ("load_spec","select_cases"):
                out=search(name, file)
                rows[name]={"header":out.splitlines()[0],"locations":[s for s in out.splitlines() if s.startswith(file+":")],
                             "navigation":[s for s in out.splitlines() if s.startswith("[navigation]")]}
        assert (root/file).read_bytes() == data
    return {"binary_sha256":digest(binary.read_bytes()),"source_sha256":digest(data),"searches":rows,"external_requests":0}


if __name__ == "__main__":
    p=argparse.ArgumentParser(description=__doc__)
    p.add_argument("binary",type=Path);p.add_argument("sealed_r10",type=Path)
    a=p.parse_args(); print(json.dumps(run(a.binary.resolve(),a.sealed_r10.resolve()),indent=2))
