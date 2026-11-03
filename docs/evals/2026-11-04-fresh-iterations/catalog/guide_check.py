"""Run a guide's literal positive catalog examples through the assembled local npm package."""
import hashlib
import json
import os
from pathlib import Path
import re
import shlex
import shutil
import subprocess
import sys
import tempfile


def measure(candidate, binary):
    doc = (candidate / "docs/evaluation.md").read_text()
    cases = re.findall(r"^\x60\x60\x60console\n\$ (spiral tui-replay-list[^\n]*)\n(\{[^\n]*\})\n\x60\x60\x60$", doc, re.M)
    rows = []
    with tempfile.TemporaryDirectory() as tmp:
        area = Path(tmp)
        env = {k: os.environ[k] for k in ("PATH", "RUSTUP_HOME", "CARGO_HOME", "TMPDIR", "SYSTEMROOT") if k in os.environ}
        host = re.search(r"^host: (\S+)$", subprocess.check_output(["rustc", "-vV"], text=True), re.M)[1]
        vendored = area / "artifacts" / host / binary.name
        vendored.parent.mkdir(parents=True)
        shutil.copy2(binary, vendored)
        package = area / "package"
        subprocess.run(["node", "scripts/prepare-npm.mjs", "--input", area / "artifacts", "--out", package,
                        "--local-target", host], cwd=candidate, env=env, check=True, capture_output=True, timeout=45)
        digest = hashlib.sha256(binary.read_bytes()).hexdigest()
        assert json.loads((package / "native-manifest.json").read_text())["binaries"][host]["sha256"] == digest
        entry = package / json.loads((package / "package.json").read_text())["bin"]["spiral"]
        project = area / "project"
        spec = project / ".spiral-coder/tui_replay.json"
        spec.parent.mkdir(parents=True)
        shutil.copy2(candidate / ".spiral-coder/tui_replay.json", spec)
        allowed = {"--tool-root": r"\.", "-C": r"\.", "--spec": r"(?:\./)?\.spiral-coder/tui_replay\.json",
                   "--filter": r"[\w.-]{1,100}", "--max-cases": r"[0-9]{1,4}"}
        for command, shown in cases:
            args = shlex.split(command)
            opts = args[2:]
            assert args[:2] == ["spiral", "tui-replay-list"] and len(opts) % 2 == 0
            assert all(k in allowed and re.fullmatch(allowed[k], v) for k, v in zip(opts[::2], opts[1::2]))
            p = subprocess.run(["node", entry, *args[1:]], cwd=project, env=env, capture_output=True, text=True, timeout=30)
            try:
                expected, actual = json.loads(shown), json.loads(p.stdout)
            except ValueError:
                expected, actual = shown, p.stdout
            rows.append(dict(command=command, shown=expected, actual=actual, exit=p.returncode, stderr=p.stderr,
                             matches=p.returncode == 0 and expected == actual))
    return dict(candidate_binary_sha256=digest, examples=rows, matches=bool(rows) and all(r["matches"] for r in rows))


if __name__ == "__main__":
    print(json.dumps(measure(*(Path(a).resolve() for a in sys.argv[1:])), indent=2))
