"""Keep the original case and add two visible checks derived from public CLI behavior."""
import json
from pathlib import Path
import shlex
import sys

HERE = Path(__file__).resolve().parent
REPO = HERE.parents[3]
sys.path.insert(0, str(REPO / "benchmarks/self-dogfood-v2/replay-catalog"))
from materialize import materialize

BIN = 'b=str(pathlib.Path(os.environ["CARGO_TARGET_DIR"])/"debug"/"spiral-coder");'
BUILD = 'subprocess.run(["cargo","build","--locked","--quiet","--bin","spiral-coder"],check=True);'
IMPORTS = 'import json,os,pathlib,subprocess,tempfile;'
PREFIX = IMPORTS + BUILD + BIN
GUIDE_COMMAND = "spiral tui-replay-list --filter prefs --max-cases 2"
EMPTY = ''.join([PREFIX, 't=tempfile.TemporaryDirectory();',
    'args=["--tool-root",os.getcwd(),"--filter","spiral-public-selection-absent-8547"];',
    'a=subprocess.run([b,"--provider","openai","tui-replay-list",*args],capture_output=True);',
    'z=subprocess.run([b,"--provider","openai","tui-replay",*args,"--out-dir",t.name],capture_output=True);',
    'print("listing:",a.returncode,a.stderr.decode());print("ordinary:",z.returncode,z.stderr.decode());',
    'assert a.returncode==z.returncode!=0 and a.stdout==z.stdout==b"" and z.stderr.strip() and a.stderr==z.stderr'])
GUIDE = ''.join([PREFIX,
    'v=subprocess.run([b,"--provider","openai","tui-replay-list","--filter","prefs","--max-cases","2"],capture_output=True,text=True,check=True);',
    'print("candidate output:",v.stdout.strip());',
    'text=pathlib.Path("docs/evaluation.md").read_text();',
    'marker="$ spiral tui-replay-list --filter prefs --max-cases 2\\n";',
    'assert text.count(marker)==1; shown=json.loads(text.split(marker)[1].splitlines()[0]);',
    'actual=json.loads(v.stdout); assert shown==actual and actual["total"]>0 and actual["total"]==len(actual["cases"])'])
PUBLIC_COMMANDS = [shlex.join(["python3", "-S", "-c", code]) for code in (EMPTY, GUIDE)]


def prepare(out):
    materialize(out, order_guidance=True, reuse_guidance=True, route_guidance=True)
    path = out / "prompt.txt"
    prefix, tree = path.read_text().split("<task_requirements>")
    original, suffix = tree.split("</task_requirements>")
    req = json.loads(original)
    req["requirements"][0]["checks"].extend({"command": cmd} for cmd in PUBLIC_COMMANDS)
    req["requirements"][0]["checks"].append({"command": "cargo fmt --all -- --check"})
    instructions = """The operator will exercise two public behaviors after delivery. Before you add your own error handling, locate the ordinary replay's selection, including its no-match branch and the existing diagnostic it returns. The listing and the ordinary replay must use the same selection behavior and same detailed error for the same input. Keep the listing free of replay output creation. Exercise this comparison with the two public commands on the stock spec. No hard-coded diagnostic wording is needed.

Append one new section at the very end of docs/evaluation.md, leaving the original guide bytes in place. The section needs a literal successful console example. First run the actual candidate with --filter prefs --max-cases 2 against the checked-in spec. In a console fence, put '$ spiral tui-replay-list --filter prefs --max-cases 2' on its own line, followed immediately by the actual one-line JSON from that invocation. The two examples in this request are operator inputs. Measure the results from your working tree.

The declared public checks below rebuild the candidate natively. One compares the two error paths. The other compares your printed guide example to the positive command. Use the available check IDs when offered. After final edits, verify the original visible check, Rust suite, both public checks and formatting. Explain the measured outcomes and list the original two native commands in the final report. If you run out of turns, hand off the next concrete action truthfully.
"""
    path.write_text(prefix.rstrip() + "\n\n" + instructions + "\n<task_requirements>\n" +
                    json.dumps(req, indent=2) + "\n</task_requirements>" + suffix)
    spec = out / "luna-luna.json"
    cfg = json.loads(spec.read_text())
    cfg["wall_seconds"] = 1800
    spec.write_text(json.dumps(cfg, indent=2) + "\n")


if __name__ == "__main__":
    prepare(Path(sys.argv[1]).resolve())
