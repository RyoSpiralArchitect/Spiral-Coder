"""Preserve baseline test identities while running every current local check."""
import json,re,subprocess,sys
from pathlib import Path
receipts=Path(sys.argv[1])
p=subprocess.run(['bash','scripts/check-local.sh'],stdout=subprocess.PIPE,stderr=subprocess.STDOUT,text=True)
print(p.stdout)
if p.returncode: raise SystemExit(p.returncode)
names=sorted(re.findall(r'^test (\S+) \.\.\. (ok|ignored)\b',p.stdout,re.M))
(receipts/'rust-tests.json').write_text(json.dumps(names,separators=(',',':')))
original=receipts.parents[1]/'baseline/artifacts/rust-tests.json'
if original != receipts/'rust-tests.json':
    assert original.is_file(), 'missing baseline test identities'
    assert not set(map(tuple,json.loads(original.read_text()))) - set(names),'original test identity/status lost'
assert len(names)>=1156, len(names)
assert re.search(r'^Ran 97 tests.*\n\s*OK\b',p.stdout,re.M)
assert re.findall(r'^[ℹ#]\s+pass (\d+)$',p.stdout,re.M)==['80','5']
assert '[tui-replay] summary: 20/20 passed' in p.stdout
assert 'E2E smoke OK: local assets + status + git exec' in p.stdout
assert any(r.get('passed')==r.get('cases')==14 and r.get('ok') is True for r in
           (json.loads(s) for s in p.stdout.splitlines() if s.startswith('{"ok"')))

original_doc=receipts.parents[2]/'baseline/docs/state-schema.md'
assert Path('docs/state-schema.md').read_bytes().startswith(original_doc.read_bytes()), 'existing state schema documentation changed'
