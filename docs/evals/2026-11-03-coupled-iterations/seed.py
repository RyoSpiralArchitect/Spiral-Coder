"""Migrate the sealed legacy R18 handoff without supplying a repair."""
import argparse
import json
from pathlib import Path
import shutil
import subprocess
import sys

HERE = Path(__file__).resolve().parent
REPO = HERE.parents[2]
R18 = HERE.parent / "2026-11-03-strategy-memory/r18"
sys.path.insert(0, str(REPO / "scripts"))
from coupled_bench.common import inventory, read_json, sha, write_json
from self_dogfood.evidence import bound_run


def prepare(parent, out):
    if out.exists() or out.is_relative_to(parent) or out.is_relative_to(REPO):
        raise ValueError("use a new external working directory")
    plan, result, audit = [read_json(R18 / f) for f in ("plan.json", "results.json", "terminal-audit.json")]
    assert bound_run(parent, plan["declaration_sha256"], result["run_receipt_sha256"])
    assert sha(parent / "predeclared.json") == plan["declaration_sha256"]
    assert sha(parent / "episode/candidate.patch") == read_json(R18 / "plain-replay.json")["patch_sha256"]
    old = parent / "episode/workspace"
    out.mkdir()
    root = out / "workspace"
    subprocess.run(["git", "clone", "--quiet", "--no-local", str(old), root], check=True)
    subprocess.run(["git", "remote", "remove", "origin"], cwd=root, check=True)
    base = read_json(parent/"predeclared.json")["candidate_base_commit"]
    # The native pre-session checkpoint is an empty commit on the assembled tree.
    def rev(ref):
        return subprocess.check_output(["git", "rev-parse", ref], cwd=root, text=True).strip()
    assert rev("HEAD") == read_json(parent/"episode/run/session.json")["checkpoint"]
    assert rev("HEAD^{tree}") == rev(base + "^{tree}") == plan["matched_candidate_tree"]
    subprocess.run(["git", "apply", "--binary", parent / "episode/candidate.patch"], cwd=root, check=True)
    subprocess.run(["git", "add", "-N", "."], cwd=root, check=True)
    before = inventory(root, include_nested_git=True)
    assert before == read_json(parent / "episode/candidate-inventory.json")
    write_json(out / "initial-inventory.json", before)
    report = read_json(parent / "episode/run/report.json")
    reply = next(m["raw_content"] for m in reversed(report["observer_display"]) if m["role"] == "Assistant")
    assert reply.encode() == (R18 / "terminal-observer.json").read_bytes()
    assert sha(R18 / "terminal-observer.json") == audit["original_reply_sha256"]
    assert audit["native_issue"] is None and not audit["queued_hint"]
    envelope = json.loads(reply)
    entry, = envelope["suggestions"]
    assert entry["kind"] == "clarify" and entry["suggested_tool"] is None and not envelope["response_contract"]["required"]
    session_path = parent / "episode/run/session.json"
    sess = json.loads(session_path.read_text().replace(str(old), str(root)))
    active = next(m for m in reversed(sess["messages"]) if m["role"] == "user" and m.get("origin") != "runtime")
    active["iteration_handoff"] = dict(version=1, source_task_id=report["work"]["task_id"],
                                      summary=envelope["summary"][:600], future_action=entry["reason"][:800])
    write_json(out / "seed.json", sess)
    shutil.copyfile(parent / "episode/task.txt", out / "task.txt")
    assert sha(out/"task.txt") == plan["prompt_sha256"]
    return dict(r18_declaration=plan["declaration_sha256"], r18_seal=result["run_receipt_sha256"],
                r18_task_id=report["work"]["task_id"], r18_session_sha256=sha(session_path),
                validated_observer_reply_sha256=audit["original_reply_sha256"], base=base,
                seed_sha256=sha(out / "seed.json"), inventory_sha256=sha(out / "initial-inventory.json"),
                prompt_sha256=sha(out / "task.txt"))


if __name__ == "__main__":
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("parent", type=Path)
    parser.add_argument("out", type=Path)
    args = parser.parse_args()
    print(json.dumps(prepare(args.parent.resolve(), args.out.resolve()), indent=2))
