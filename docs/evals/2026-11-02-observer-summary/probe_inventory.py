"""Post-run control: does the inventory entry proposed at R7 step 22 register a CLI command?"""
import argparse
import fcntl
import hashlib
import json
import os
from pathlib import Path
import subprocess
import tempfile

HERE = Path(__file__).resolve().parent
COMMAND = "tui-replay-list"


def sha(data):
    return hashlib.sha256(data).hexdigest()


def probe(repo, episode, target):
    manifest = json.loads((HERE / "receipts.json").read_bytes())["files"]

    def pinned(path):
        data = (episode / path).read_bytes()
        if sha(data) != manifest[path]["sha256"]:
            raise ValueError(f"unbound {path}")
        return data

    report = json.loads(pinned("episode/run/report.json"))
    action, = [a for a in report["execution"]["actions"] if a["result_step"] == 22]
    assert action["tool"] == "apply_diff" and action["executed"] is False
    session = json.loads(pinned("episode/run/session.json"))
    call, = [c for m in session["messages"] for c in m.get("tool_calls", []) if c["id"] == action["call_id"]]
    args = json.loads(call["function"]["arguments"])
    assert args["path"] == "src/main.rs"
    removed = [line[1:] for line in args["diff"].splitlines() if line.startswith("-")]
    added = [line[1:] for line in args["diff"].splitlines() if line.startswith("+")]
    assert len(removed) == 1 and len(added) == 2 and added[0] == removed[0] and COMMAND in added[1]
    patch = pinned("episode/candidate.patch")
    assert sha(patch) == sha((HERE / "candidate.patch").read_bytes())
    baseline = json.loads((HERE / "plan.json").read_bytes())["baseline_commit"]
    env = {k: os.environ[k] for k in ("PATH", "HOME", "RUSTUP_HOME", "CARGO_HOME") if k in os.environ}
    env.update(CARGO_TARGET_DIR=str(target), CARGO_INCREMENTAL="0", CARGO_NET_OFFLINE="true",
               CARGO_PROFILE_DEV_DEBUG="0", CARGO_PROFILE_TEST_DEBUG="0")
    target.mkdir(parents=True, exist_ok=True)
    rows = []
    with tempfile.TemporaryDirectory() as tmp, (target / ".self-dogfood.lock").open("a") as lock:
        fcntl.flock(lock, fcntl.LOCK_EX | fcntl.LOCK_NB)
        root = Path(tmp)
        with tempfile.TemporaryFile() as archive:
            subprocess.run(["git", "archive", baseline], cwd=repo, stdout=archive, check=True)
            archive.seek(0)
            subprocess.run(["tar", "-xf", "-"], cwd=root, stdin=archive, check=True)
        subprocess.run(["git", "apply", "--binary", "-"], cwd=root, input=patch, check=True)
        main = root / args["path"]
        original = main.read_bytes()
        anchor = (removed[0] + "\n").encode()
        assert original.count(anchor) == 1
        for label in ("sealed_candidate", "candidate_plus_inventory_row"):
            if label == "candidate_plus_inventory_row":
                main.write_bytes(original.replace(anchor, anchor + (added[1] + "\n").encode()))
            for cmd in [["cargo", "clean", "--locked", "--offline", "-p", "spiral-coder"],
                        ["cargo", "build", "--locked", "--offline", "--bin", "spiral-coder"]]:
                subprocess.run(cmd, cwd=root, env=env, check=True, stdout=subprocess.DEVNULL)
            binary = target / "debug/spiral-coder"

            def invoke(*argv):
                return subprocess.run([binary, *argv], cwd=root, env=env, capture_output=True, timeout=20)

            inventory = invoke("inventory", "commands", "--json")
            inventory.check_returncode()
            help_output = invoke("--help")
            help_output.check_returncode()
            direct = invoke(COMMAND, "--help")
            rows.append({"condition": label, "main_sha256": sha(main.read_bytes()),
                         "binary_sha256": sha(binary.read_bytes()), "build_exit": 0,
                         "listed_in_inventory": COMMAND in [row["name"] for row in json.loads(inventory.stdout)["cli"]],
                         "listed_in_help": any(line.split()[:1] == [COMMAND] for line in help_output.stdout.decode().splitlines()),
                         "direct_help_exit": direct.returncode, "command_unrecognized": b"unrecognized subcommand" in direct.stderr})
    return {"experiment": "operator post-run control; only the authored inventory row was added in a disposable copy",
            "baseline_commit": baseline, "candidate_patch_sha256": sha(patch), "blocked_step": 22,
            "authored_diff_sha256": sha(args["diff"].encode()), "external_provider_requests": 0, "conditions": rows}


if __name__ == "__main__":
    parser = argparse.ArgumentParser(description=__doc__)
    for name in ("repo", "episode", "target"):
        parser.add_argument(name, type=Path)
    args = parser.parse_args()
    print(json.dumps(probe(args.repo.resolve(), args.episode.resolve(), args.target.resolve()), indent=2))
