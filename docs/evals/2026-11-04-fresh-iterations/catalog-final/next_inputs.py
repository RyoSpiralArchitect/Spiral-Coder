"""Materialize the public task with child diagnostics shown only on an actual mismatch."""
import json
from pathlib import Path
import runpy
import shlex
import sys

HERE = Path(__file__).resolve().parent
prior = runpy.run_path(str(HERE.parent / "catalog-public/prepare.py"))
# Preserve the actual measurements and all five predicates from the original assertion.
measurement, condition = prior["EMPTY"].split(
    'print("listing:",a.returncode,a.stderr.decode());print("ordinary:",z.returncode,z.stderr.decode());assert ')
QUIET_COMPARISON = (measurement + "assert " + condition +
    ', {"listing":(a.returncode,a.stdout,a.stderr),"ordinary":(z.returncode,z.stdout,z.stderr)};'
    'print("No-match exit status, stdout and diagnostic matched.")')
QUIET_COMMAND = shlex.join(["python3", "-S", "-c", QUIET_COMPARISON])


def materialize(out):
    prior["prepare"](out)
    path = out / "prompt.txt"
    prefix, body = path.read_text().split("<task_requirements>")
    spec, suffix = body.split("</task_requirements>")
    contract = json.loads(spec)
    rows = [c for r in contract["requirements"] for c in r["checks"] if c["command"] == prior["PUBLIC_COMMANDS"][0]]
    if len(rows) != 1:
        raise ValueError("Expected exactly one public no-match comparator")
    rows[0]["command"] = QUIET_COMMAND
    path.write_text(prefix + "<task_requirements>\n" + json.dumps(contract, indent=2) + "\n</task_requirements>" + suffix)


if __name__ == "__main__":
    materialize(Path(sys.argv[1]).resolve())
