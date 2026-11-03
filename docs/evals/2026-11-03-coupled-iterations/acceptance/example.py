"""Run the guide's literal replay-list example through an offline host package."""
import argparse
import hashlib
import json
import os
from pathlib import Path
import re
import shlex
import shutil
import subprocess
import tempfile


def check(repo, binary):
    body=(repo/"docs/evaluation.md").read_text()
    examples=re.findall(r"^```console\n\$ (spiral tui-replay-list[^\n]*)\n([^\n]+)\n```$",body,re.M)
    if len(examples)!=1:
        raise ValueError("expected one documented catalog JSON console example")
    command,printed=examples[0]
    args=shlex.split(command)
    if args!=["spiral","tui-replay-list","--tool-root",".","--filter","observer","--max-cases","1"]:
        raise ValueError("documented public example command changed")
    expected=json.loads(printed)
    with tempfile.TemporaryDirectory() as tmp:
        area=Path(tmp)
        env={k:os.environ[k] for k in ("PATH","SYSTEMROOT","RUSTUP_HOME","CARGO_HOME","TMPDIR") if k in os.environ}
        host=re.search(r"^host: (\S+)$",subprocess.check_output(["rustc","-vV"],text=True),re.M)[1]
        artifact=area/"input"/host/binary.name;artifact.parent.mkdir(parents=True);shutil.copyfile(binary,artifact)
        pkg=area/"package"
        subprocess.run(["node","scripts/prepare-npm.mjs","--input",area/"input","--out",pkg,"--local-target",host],cwd=repo,env=env,capture_output=True,check=True,timeout=50)
        digest=hashlib.sha256(binary.read_bytes()).hexdigest()
        assert json.loads((pkg/"native-manifest.json").read_bytes())["binaries"][host]["sha256"]==digest
        entry=pkg/json.loads((pkg/"package.json").read_bytes())["bin"]["spiral"]
        p=subprocess.run(["node",entry,*args[1:]],cwd=repo,env=env,capture_output=True,text=True,timeout=30)
        actual=json.loads(p.stdout) if p.returncode==0 else None
        return dict(command=command,binary_sha256=digest,exit=p.returncode,expected=expected,actual=actual,
                    matches=p.returncode==0 and actual==expected,stderr=p.stderr[:400])


if __name__=="__main__":
    p=argparse.ArgumentParser(description=__doc__)
    p.add_argument("repo",type=Path);p.add_argument("binary",type=Path)
    opts=p.parse_args()
    print(json.dumps(check(opts.repo.resolve(),opts.binary.resolve()),indent=2))
