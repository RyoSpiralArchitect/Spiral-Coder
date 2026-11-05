"""Run the original full check and pin the names and statuses of its Rust tests."""
import hashlib
import json
from pathlib import Path
import re
import subprocess
import sys

process = subprocess.run(["bash", "scripts/check-local.sh"], stdout=subprocess.PIPE,
                         stderr=subprocess.STDOUT, text=True)
print(process.stdout)
if process.returncode:
    raise SystemExit(process.returncode)
log = process.stdout
names = sorted(re.findall(r"^test (\S+) \.\.\. (ok|ignored)\b", log, re.M))
receipt = Path(sys.argv[1]) / "rust-tests.json"
receipt.write_text(json.dumps(names, separators=(",", ":")))
if hashlib.sha256(receipt.read_bytes()).hexdigest() != "52e6a32caa6c5ecc30d576da2c95846a4c0bb9d431a2f0e1262f68d659f41033":
    raise ValueError("original Rust test identities or statuses changed")
if not (re.search(r"^Ran 97 tests.*\n\s*OK\b", log, re.M)
        and re.findall(r"^[ℹ#]\s+pass (\d+)$", log, re.M) == ["80", "5"]
        and "[tui-replay] summary: 15/15 passed" in log
        and any(row.get("ok") is True and row.get("passed") == row.get("cases") == 14
                for row in (json.loads(line) for line in log.splitlines() if line.startswith('{"ok"')))
        and "E2E smoke OK:" in log):
    raise ValueError("stock non-Rust checks did not match the frozen baseline")
