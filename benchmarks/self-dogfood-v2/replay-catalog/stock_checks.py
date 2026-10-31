"""Run the repository's original check entry point in the disposable verifier."""
import hashlib
import json
from pathlib import Path
import re
import subprocess
import sys

RECEIPTS = Path(sys.argv[1])
STOCK_HASH = "03ee93445176ec84dfd9e87e64461b9151ccdf14f27e3934c488a48780240dc6"
process = subprocess.run(["bash", "scripts/check-local.sh"], stdout=subprocess.PIPE,
                         stderr=subprocess.STDOUT, text=True)
print(process.stdout)
if process.returncode:
    raise SystemExit(process.returncode)
log = process.stdout
names = sorted(re.findall(r"^test (\S+) \.\.\. (ok|ignored)\b", log, re.M))
(RECEIPTS / "rust-tests.json").write_text(json.dumps(names, separators=(",", ":")))
original = RECEIPTS.parents[1] / "baseline/artifacts/rust-tests.json"
assert hashlib.sha256(original.read_bytes()).hexdigest() == STOCK_HASH, "baseline Rust test identities changed"
assert not set(map(tuple, json.loads(original.read_text()))) - set(names), "an original Rust result was lost"
assert re.search(r"^Ran 93 tests.*\n\s*OK\b", log, re.M)
counts = re.findall(r"^[ℹ#]\s+pass (\d+)$", log, re.M)
assert counts == ["80", "5"], counts
assert "[tui-replay] summary: 13/13 passed" in log
assert any(row.get("passed") == row.get("cases") == 14 and row.get("ok") is True for row in
           (json.loads(line) for line in log.splitlines() if line.startswith('{"ok"')))
assert "E2E smoke OK: local assets + status + git exec" in log
