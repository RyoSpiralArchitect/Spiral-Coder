"""Replay the sealed r3 diff inputs on the original and replacement native file tools."""
import argparse
import json
from pathlib import Path
import sys
import tempfile

HERE = Path(__file__).resolve().parent
PRIOR = HERE.parent / "order-r3-routing"
sys.path.insert(0, str(PRIOR))
from replay_hunks import digest, file_server, retained_inputs, PARTIAL, PATH, REJECTED
sys.path.insert(0, str(HERE.parents[3] / "scripts"))
from self_dogfood.evidence import bound_run


def compare(raw, binary, expected_sha):
    old_plan, old_binary, original, calls = retained_inputs(raw)
    assert digest(binary.read_bytes()) == expected_sha
    prior = json.loads((PRIOR / "hunk-replay.json").read_bytes())
    with tempfile.TemporaryDirectory() as temp:
        root = Path(temp).resolve()
        path = root / PATH
        path.parent.mkdir()
        path.write_bytes(original)
        with file_server(old_binary, root) as apply:
            code, old_message = apply(calls[PARTIAL])
            assert code == 200 and old_message.startswith("OK: applied 3/5")
            before_retry = path.read_bytes()
        assert digest(before_retry) == prior["pre_edit_source_sha256"]
        with file_server(binary, root) as apply:
            path.write_bytes(original)
            code, message = apply(calls[PARTIAL])
            assert code == 400 and path.read_bytes() == original
            partial = {"call_id": PARTIAL, "old_result": old_message.splitlines()[0],
                       "fixed_status": code, "fixed_result": message.splitlines()[0], "unchanged": True}
            results = []
            for call_id, old in zip(REJECTED, prior["results"], strict=True):
                path.write_bytes(before_retry)
                args = calls[call_id]
                assert old["call_id"] == call_id and digest(args["diff"].encode()) == old["original_diff_sha256"]
                code, message = apply(args)
                result = path.read_bytes()
                assert code == 200 and "ignored trailing `*** End Patch`" in message
                assert digest(result) == old["trimmed_result_sha256"]
                results.append({"call_id": call_id, "request_diff_sha256": old["original_diff_sha256"],
                                "old_status": old["original_status"], "fixed_status": code,
                                "fixed_result": message, "output_sha256": digest(result),
                                "matches_explicit_footer_removal": True,
                                "resolve_path_definitions_after": result.count(b"fn resolve_path(")})
    return {"provider_calls": 0, "old_executor_sha256": old_plan["executor_sha256"],
            "new_executor_sha256": expected_sha, "prior_source_sha256": digest(before_retry),
            "initial_multi_hunk_edit": partial, "rejected_retries_on_historical_input": results}


def compare_registration(raw, receipt, binary, expected_sha):
    """The first edited path in r4 was a one-line module registration."""
    plan = json.loads((HERE.parent / "reuse-r4/plan.json").read_bytes())
    declared = (raw / "predeclared.json").read_bytes()
    assert digest(declared) == plan["declaration_sha256"] and bound_run(raw, plan["declaration_sha256"], receipt)
    original = (raw / "baseline" / PATH).read_bytes()
    assert digest(original) == json.loads(declared)["baseline_inventory"][PATH]["sha256"]
    report = json.loads((raw / "episode/run/report.json").read_bytes())
    actions = [a for a in report["execution"]["actions"] if a.get("executed")]
    assert [a["tool"] for a in actions[:4]] == ["exec", "read_file", "read_file", "apply_diff"]
    assert actions[0]["arguments"]["command"] == "python3 -S -m unittest -v scripts.catalog_visible"
    call = actions[3]
    request = call["arguments"]
    assert request == {"path": PATH, "diff": "@@\n pub(crate) mod diagnostics;\n+pub(crate) mod catalog;\n*** End Patch"}
    old_binary = raw / "runtime/spiral-coder"
    assert digest(old_binary.read_bytes()) == plan["executor_sha256"] and digest(binary.read_bytes()) == expected_sha
    anchor = b"pub(crate) mod diagnostics;\n"
    assert original.count(anchor) == 1
    expected = original.replace(anchor, anchor + b"pub(crate) mod catalog;\n")
    results = []
    with tempfile.TemporaryDirectory() as temp:
        root = Path(temp).resolve()
        path = root / PATH
        path.parent.mkdir()
        for executable, status, output in [(old_binary, 400, original), (binary, 200, expected)]:
            path.write_bytes(original)
            with file_server(executable, root) as apply:
                code, message = apply(request)
            assert code == status and path.read_bytes() == output
            results.append({"status": code, "message": message, "output_sha256": digest(output)})
    return {"provider_calls": 0, "call_id": call["call_id"], "run_receipt_sha256": receipt,
            "old_executor_sha256": plan["executor_sha256"], "new_executor_sha256": expected_sha,
            "request": request, "old": results[0], "fixed": results[1], "only_change": "inserted pub(crate) mod catalog;"}


if __name__ == "__main__":
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("episode", type=Path)
    parser.add_argument("executor", type=Path)
    parser.add_argument("--sha256", required=True)
    parser.add_argument("--r4-registration", metavar="RUN_RECEIPT_SHA256",
                        help="Compare the r4 module registration instead of the r3 helper edits")
    args = parser.parse_args()
    root, binary = args.episode.resolve(), args.executor.resolve()
    result = (compare_registration(root, args.r4_registration, binary, args.sha256) if args.r4_registration
              else compare(root, binary, args.sha256))
    print(json.dumps(result, indent=2))
