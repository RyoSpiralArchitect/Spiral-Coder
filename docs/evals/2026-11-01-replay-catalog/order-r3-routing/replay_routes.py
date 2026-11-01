"""Replay the two recorded r2 verification calls at the follow-up and module-discovery stages."""
import argparse
import json
from pathlib import Path
import subprocess
import sys
import tempfile

HERE = Path(__file__).resolve().parent
REPO = HERE.parents[3]
sys.path.insert(0, str(REPO / "scripts"))
from coupled_bench.common import clean_env, read_json, sha, write_json
from self_dogfood.contracts import git_snapshot
from self_dogfood.runner import cache_lock


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("episode", type=Path)
    parser.add_argument("output", type=Path)
    args = parser.parse_args()
    raw, output = args.episode.resolve(), args.output.resolve()
    if output.exists() or output.is_relative_to(raw) or output.is_relative_to(REPO):
        parser.error("Use a new external output directory")
    # The episode/candidate is read only. A fresh verifier builds each revision.
    digests = read_json(HERE.parent / "order-r2/receipts.json")["files"]
    for name in ("episode/run/session.json", "episode/run/trace.jsonl"):
        if sha(raw / name) != digests[name]["sha256"]:
            parser.error("The retained session or trace differs from r2")
    declaration = read_json(raw / "predeclared.json")
    if (sha(raw / "predeclared.json") != read_json(HERE.parent / "order-r2/plan.json")["declaration_sha256"]
            or sha(raw / "layout.json") != declaration["layout_sha256"]):
        parser.error("The retained layout differs from r2")
    trace = [json.loads(line) for line in (raw / "episode/run/trace.jsonl").read_text().splitlines()]
    ids = [next(r["data"]["id"] for r in trace[i+1:] if r["event"] == "tool_call")
           for i, r in enumerate(trace) if r["event"] == "task_harness_tool_coercion"
           and r["data"]["from"] == "cargo test 2>&1" and r["data"]["reason"] == "required_followup_edit"]
    if len(ids) != 2 or len(set(ids)) != 2:
        parser.error("Expected the two distinct r2 redirects")
    plan = read_json(HERE / "plan.json")
    target = Path(read_json(raw / "layout.json")["target_dir"])
    output.mkdir(parents=True)
    rows = []
    env = dict(clean_env(), CARGO_TARGET_DIR=str(target), CARGO_INCREMENTAL="0", CARGO_PROFILE_DEV_DEBUG="0",
               CARGO_PROFILE_TEST_DEBUG="0", CATALOG_SAVED_SESSION=str(raw/"episode/run/session.json"),
               CATALOG_CALL_IDS=json.dumps(ids), CATALOG_CANDIDATE_ROOT=str(raw/"episode/workspace"))
    with cache_lock(target):
        for label, revision in (("original", plan["baseline_commit"]), ("fixed", plan["runtime_commit"])):
            with tempfile.TemporaryDirectory(dir=output) as temp:
                source = Path(temp) / "source"
                git_snapshot(REPO, revision, source)
                with (source / "src/tui/agent/followup_edit_safety_tests.rs").open("ab") as f:
                    f.write((HERE / "routing_probe.rs").read_bytes())
                subprocess.run(["cargo", "clean", "--locked", "--offline", "--package", "spiral-coder"],
                               cwd=source, env=env, capture_output=True, check=True, timeout=60)
                with (output / (label + ".log")).open("w") as log:
                    subprocess.run(["cargo","test","--locked","--bin","spiral-coder","catalog_retained_verification_route",
                                    "--","--nocapture"],cwd=source, env=env, stdout=log,
                                   stderr=subprocess.STDOUT, check=True, timeout=180)
            log = output / (label + ".log")
            routes = [json.loads(line[6:]) for line in log.read_text().splitlines() if line.startswith("ROUTE ")]
            if [r["call_id"] for r in routes] != ids:
                raise RuntimeError("The probe did not replay both retained prefixes")
            rows.append({"revision":revision,"label":label,"routes":routes,"log_sha256":sha(log)})
    write_json(output / "results.json", {"session_sha256":digests["episode/run/session.json"]["sha256"],
               "trace_sha256":digests["episode/run/trace.jsonl"]["sha256"], "probe_sha256":sha(HERE/"routing_probe.rs"),
               "calls":ids,"provider_calls":0,"revisions":rows})


if __name__ == "__main__":
    main()
