"""Extract descriptive edit and Observer evidence for manual review of a sealed episode."""
import argparse
import difflib
import hashlib
import json
from pathlib import Path
import re
import sys

REPO = Path(__file__).resolve().parents[4]
sys.path.insert(0, str(REPO / "scripts"))
from coupled_bench.common import read_json, sha
from self_dogfood.evidence import bound_run

HELPERS = ("resolve_path", "load_spec", "validate_case")


def observe(root, declaration, receipt):
    if sha(root / "predeclared.json") != declaration or not bound_run(root, declaration, receipt):
        raise ValueError("Missing sealed episode")
    declared = read_json(root / "predeclared.json")
    spec_path = root / "inputs/spec.json"
    if sha(spec_path) != declared["inputs_inventory"]["spec.json"]["sha256"]:
        raise ValueError("Task scope differs from declaration")
    spec = read_json(spec_path)
    episode = root / "episode"
    report = read_json(episode / "run/report.json")
    actions = {a["call_id"]: a for a in report["execution"]["actions"] if a.get("call_id")}
    diffs = []
    definitions = []
    for line in (episode / "run/trace.jsonl").read_text().splitlines():
        row = json.loads(line)
        if row["event"] != "tool_call" or row["data"].get("name") not in ("apply_diff", "patch_file", "write_file"):
            continue
        call = row["data"]
        action = actions.get(call["id"], {})
        try:
            args = json.loads(call["arguments"])
        except (TypeError, ValueError):
            args = None
        args = args if isinstance(args, dict) else {}
        diff = args.get("diff")
        lines = diff.splitlines() if isinstance(diff, str) else []
        if call["name"] == "apply_diff":
            proposed = "\n".join(s[1:] for s in lines if s.startswith("+") and not s.startswith("+++"))
        else:
            proposed = args.get("replace" if call["name"] == "patch_file" else "content")
        for helper in HELPERS:
            if isinstance(proposed, str) and re.search(r"\bfn\s+" + helper + r"\s*\(", proposed):
                definitions.append({"call_id": call["id"], "tool": call["name"], "path": args.get("path"),
                                    "helper": helper, "executed": action.get("executed"),
                                    "edit_outcome": action.get("edit_outcome")})
        if call["name"] != "apply_diff":
            continue
        message = action.get("result") or ""
        diffs.append({"call_id": call["id"], "path": args.get("path"), "executed": action.get("executed"),
                      "bare_trailer": lines[-1:] == ["*** End Patch"] if isinstance(diff, str) else None,
                      "edit_outcome": action.get("edit_outcome"),
                      "repair_receipt": "Format repair: ignored trailing `*** End Patch`." in message,
                      "native_first_line": (message.splitlines() or [None])[0]})

    sites = []
    for name in spec["editable"]:
        if not name.endswith(".rs"):
            continue
        original = root / "baseline" / name
        final = episode / "workspace" / name
        if (sha(original) if original.is_file() else None) != declared["baseline_inventory"].get(name, {}).get("sha256"):
            raise ValueError(f"Original source differs from declaration: {name}")
        delta = difflib.unified_diff(original.read_text().splitlines() if original.is_file() else [],
                                     final.read_text().splitlines() if final.is_file() else [], n=0)
        for line in delta:
            if not line.startswith(("+", "-")) or line.startswith(("+++", "---")):
                continue
            symbols = [h for h in HELPERS if re.search(r"\b" + h + r"\b", line)]
            if symbols:
                sites.append({"path": name, "line": line, "symbols": symbols})

    observer = []
    for msg in report["observer_display"]:
        if msg.get("role") != "Assistant":
            continue
        raw = msg.get("raw_content") or msg.get("content") or ""
        try:
            obj = json.loads(raw)
        except ValueError:
            obj = None
        suggestions = obj.get("suggestions") if isinstance(obj, dict) else None
        first = suggestions[0] if isinstance(suggestions, list) and suggestions else None
        observer.append({"raw_sha256": hashlib.sha256(raw.encode()).hexdigest(),
                         "summary": obj.get("summary") if isinstance(obj, dict) else None,
                         "primary": {key: first.get(key) for key in ("kind","suggested_tool","reason")}
                         if isinstance(first, dict) else None})
    return {"declaration_sha256": declaration, "run_receipt_sha256": receipt, "diff_calls": diffs,
            "definitions_in_proposed_replacement_text": definitions,
            "changed_lines_mentioning_r3_helpers": sites, "observer_replies": observer}


if __name__ == "__main__":
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("episode", type=Path)
    parser.add_argument("--declaration", required=True)
    parser.add_argument("--receipt", required=True)
    args = parser.parse_args()
    print(json.dumps(observe(args.episode.resolve(), args.declaration, args.receipt), indent=2))
