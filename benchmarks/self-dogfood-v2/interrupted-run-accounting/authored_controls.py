"""Measure Windows compilation and whether authored public tests distinguish JSON semantics."""
import argparse
import hashlib
import json
import os
from pathlib import Path
import re
import shutil
import subprocess


def sha(path):
    return hashlib.sha256(path.read_bytes()).hexdigest()


def measure(source, reference, host_target, windows_target, out):
    out.mkdir(parents=True, exist_ok=False)
    test = source / "tests/coupled_status.rs"
    result = {"authored_tests_sha256": sha(test) if test.is_file() else None,
              "reference_cli_sha256": sha(reference), "tested_target":"x86_64-pc-windows-gnu",
              "windows_compile":None, "output_controls":{}, "passed":False}
    if not test.is_file():
        return result
    env = {k:os.environ[k] for k in ("PATH", "HOME", "TMPDIR", "SystemRoot",
           "CC_x86_64_pc_windows_gnu", "AR_x86_64_pc_windows_gnu") if k in os.environ}
    env.update(CARGO_INCREMENTAL="0", CARGO_PROFILE_DEV_DEBUG="0", CARGO_PROFILE_TEST_DEBUG="0",
               PYTHONPYCACHEPREFIX=str(out / "pycache"))
    scratch = out / "workspace"
    shutil.copytree(source, scratch, ignore=lambda d, names: {".git","target"} if Path(d)==source else set())
    assert sha(scratch / "tests/coupled_status.rs") == result["authored_tests_sha256"]

    def command(args, log, extra=None):
        run = subprocess.run(args, cwd=scratch, env=env | (extra or {}), stdout=subprocess.PIPE,
                             stderr=subprocess.STDOUT, text=True, timeout=360)
        (out / log).write_text(run.stdout)
        return run

    env["CARGO_TARGET_DIR"] = str(windows_target)
    command(["cargo","clean","--locked","--offline","--package","spiral-coder","--target",result["tested_target"]],"windows-clean.log").check_returncode()
    cross = command(["cargo","check","--locked","--offline","--test","coupled_status","--target",result["tested_target"]],"windows.log")
    result["windows_compile"] = {"passed":cross.returncode==0,"exit":cross.returncode,"log_sha256":sha(out/"windows.log")}

    # The integration test is byte-identical. Only its disposable CLI entry
    # point changes, forwarding to the same real reference for all five modes.
    shutil.copyfile(Path(__file__).with_name("output_fixture.rs"), scratch / "src/main.rs")
    env["CARGO_TARGET_DIR"] = str(host_target)
    command(["cargo","clean","--locked","--offline","--package","spiral-coder"],"native-clean.log").check_returncode()
    build = command(["cargo","test","--no-run","--locked","--offline","--test","coupled_status","--message-format=json"],"build.log")
    result["host_compile"]={"passed":build.returncode==0,"log_sha256":sha(out/"build.log")}
    if build.returncode:
        return result
    artifacts = [json.loads(line) for line in build.stdout.splitlines() if line.startswith("{")]
    executable, = (row["executable"] for row in artifacts if row.get("executable") and row["target"]["name"]=="coupled_status")
    for mode in ("native","pretty","swapped_task_ids","wrong_numbers","reversed_rows"):
        hits = out / (mode+".hits")
        run = command([executable,"--test-threads=1"],mode+".log",
                      {"SPIRAL_PROBE_BINARY":str(reference),"SPIRAL_PROBE_MODE":mode,"SPIRAL_PROBE_HITS":str(hits)})
        calls = len(hits.read_text().splitlines()) if hits.exists() else 0
        # A process crash or a test setup error cannot substitute for a paired assertion.
        failed = re.findall(r"^---- (\S+) stdout ----$",run.stdout,re.M)
        passed = calls>0 and (run.returncode==0 if mode in ("native","pretty") else run.returncode==101 and bool(failed))
        result["output_controls"][mode] = {"passed":passed,"exit":run.returncode,"two_row_calls":calls,
                                           "failed_tests":failed,"log_sha256":sha(out/(mode+".log"))}
    result["passed"] = result["windows_compile"]["passed"] and all(row["passed"] for row in result["output_controls"].values())
    return result


if __name__=="__main__":
    parser=argparse.ArgumentParser(description=__doc__)
    for name in ("source","reference","host_target","windows_target","out"):
        parser.add_argument(name,type=lambda s:Path(s).resolve())
    args=parser.parse_args()
    result=measure(args.source,args.reference,args.host_target,args.windows_target,args.out)
    (args.out/"result.json").write_text(json.dumps(result,indent=2)+"\n")
    print(json.dumps(result))
    raise SystemExit(int(not result["passed"]))
