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


if __name__ == "__main__":
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("r3_episode", type=Path)
    parser.add_argument("executor", type=Path)
    parser.add_argument("--sha256", required=True)
    args = parser.parse_args()
    print(json.dumps(compare(args.r3_episode.resolve(), args.executor.resolve(), args.sha256), indent=2))
