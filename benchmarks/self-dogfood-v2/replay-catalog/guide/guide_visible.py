"""Compare two literal console examples in the guide with the candidate's local npm entry."""

import hashlib
import json
import os
from pathlib import Path
import re
import shutil
import subprocess
import tempfile
import unittest

SUCCESS = "spiral tui-replay-list --filter prefs --max-cases 2"
EMPTY = "spiral tui-replay-list --filter __spiral_no_match__"


def measured(binary, repo):
    host = re.search(
        r"^host: (\S+)$", subprocess.check_output(["rustc", "-vV"], text=True), re.M
    )[1]
    with tempfile.TemporaryDirectory() as temp:
        area = Path(temp)
        vendor = area / "input" / host / binary.name
        vendor.parent.mkdir(parents=True)
        shutil.copyfile(binary, vendor)
        package = area / "package"
        node = shutil.which("node")
        subprocess.run(
            [
                node,
                repo / "scripts/prepare-npm.mjs",
                "--input",
                area / "input",
                "--out",
                package,
                "--local-target",
                host,
            ],
            cwd=repo,
            check=True,
            capture_output=True,
        )
        manifest = json.loads((package / "native-manifest.json").read_bytes())
        assert (
            manifest["binaries"][host]["sha256"]
            == hashlib.sha256(binary.read_bytes()).hexdigest()
        )
        entry = (
            package
            / json.loads((package / "package.json").read_bytes())["bin"]["spiral"]
        )
        project = area / "project"
        spec = project / ".spiral-coder/tui_replay.json"
        spec.parent.mkdir(parents=True)
        original = (repo / ".spiral-coder/tui_replay.json").read_bytes()
        spec.write_bytes(original)
        result = {}
        for name, command in [("listed", SUCCESS), ("empty", EMPTY)]:
            # Use only these two declared argv vectors, never execute Markdown.
            run = subprocess.run(
                [node, entry, *command.split()[1:]],
                cwd=project,
                env={"PATH": os.defpath, "HOME": str(area)},
                capture_output=True,
                text=True,
                timeout=25,
            )
            result[name] = {
                "exit": run.returncode,
                "stdout": run.stdout,
                "stderr": run.stderr,
            }
        assert sorted(
            p.relative_to(project).as_posix() for p in project.rglob("*")
        ) == [".spiral-coder", ".spiral-coder/tui_replay.json"]
        assert spec.read_bytes() == original
        return result


def discrepancies(doc, actual):
    section = re.search(
        r"(?ms)^## Read-only replay catalog[ \t]*\n(.*?)(?=^## |\Z)", doc
    )
    body = section[1] if section else ""
    issues = []
    if "`done.acceptance_evidence`" in body:
        issues.append(
            "the catalog heading includes an unrelated original completion paragraph"
        )
    lines = dict()
    for block in re.findall(r"(?ms)^```console[ \t]*\n(.*?)^```[ \t]*$", body):
        for command in [SUCCESS, EMPTY]:
            rows = block.strip().splitlines()
            if rows and rows[0] == "$ " + command:
                if command in lines:
                    issues.append("duplicate console example for " + command)
                lines[command] = rows[1:]
    listed = actual["listed"]
    output = json.loads(listed["stdout"])
    assert (
        listed["exit"] == 0
        and set(output) == {"version", "total", "cases"}
        and output["total"] == len(output["cases"])
    )
    try:
        printed = json.loads("\n".join(lines.get(SUCCESS, [])))
    except ValueError:
        printed = None
    if printed != output:
        issues.append(
            "printed JSON must equal "
            + json.dumps(output, ensure_ascii=False, separators=(",", ":"))
        )
    failed = actual["empty"]
    assert failed["exit"] > 0 and not failed["stdout"] and failed["stderr"].strip()
    if lines.get(EMPTY) != ["$ echo $?", str(failed["exit"])]:
        issues.append(
            "empty selection: print '$ echo $?' and its observed exit "
            + str(failed["exit"])
        )
    if "stderr" not in body.lower() or "stdout" not in body.lower():
        issues.append("describe the empty stdout and diagnostic on stderr")
    return issues


class Guide(unittest.TestCase):
    def test_console_examples_match_packaged_spiral(self):
        repo = Path.cwd()
        subprocess.run(
            ["cargo", "build", "--locked", "--quiet", "--bin", "spiral-coder"],
            cwd=repo,
            check=True,
        )
        binary = Path(os.environ["CARGO_TARGET_DIR"]).resolve() / "debug/spiral-coder"
        mismatches = discrepancies(
            (repo / "docs/evaluation.md").read_text(), measured(binary, repo)
        )
        if mismatches:
            print("catalog guide mismatch: " + " | ".join(mismatches), flush=True)
            self.fail("catalog guide mismatch")
