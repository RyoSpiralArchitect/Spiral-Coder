"""Compare both installed native subcommands on a copied public replay spec."""
import argparse
import hashlib
import json
import os
from pathlib import Path
import shutil
import subprocess
import tempfile

FILTER = "spiral-catalog-no-public-match-8547"


def snapshot(root):
    return {p.relative_to(root).as_posix(): hashlib.sha256(p.read_bytes()).hexdigest() if p.is_file()
            else "directory" for p in root.rglob("*")}


def compare(binary, source):
    with tempfile.TemporaryDirectory(prefix="spiral-public-diagnostic-") as tmp:
        root = Path(tmp) / "project"
        spec = root / ".spiral-coder/tui_replay.json"
        spec.parent.mkdir(parents=True)
        shutil.copy2(source / ".spiral-coder/tui_replay.json", spec)
        cwd = Path(tmp) / "caller"
        cwd.mkdir()
        generated = Path(tmp) / "replay-output"
        env = {"PATH": os.defpath, "HOME": tmp}
        before = snapshot(Path(tmp))
        observed = {}
        for sub in ("tui-replay-list", "tui-replay"):
            args = [str(binary), "--provider", "openai", sub, "--tool-root", str(root), "--filter", FILTER]
            if sub == "tui-replay":
                args += ["--out-dir", str(generated)]
            p = subprocess.run(args, cwd=cwd, env=env, capture_output=True, text=True, timeout=30)
            observed[sub] = dict(exit=p.returncode, stdout=p.stdout, stderr=p.stderr)
            if sub == "tui-replay-list":
                clean = before == snapshot(Path(tmp))
        listing, replay = observed.values()
        return {"filter": FILTER, "commands": observed, "listing_preserved_all_files_and_directories": clean,
                "matches": clean and listing["exit"] == replay["exit"] != 0 and
                listing["stdout"] == replay["stdout"] == "" and
                bool(replay["stderr"].strip()) and listing["stderr"] == replay["stderr"]}


if __name__ == "__main__":
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("binary", type=Path)
    parser.add_argument("source", type=Path)
    args = parser.parse_args()
    print(json.dumps(compare(args.binary.resolve(), args.source.resolve()), indent=2))
