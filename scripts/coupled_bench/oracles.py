"""Score artifacts using frozen original tests, outside the candidate workspace."""
import json
from pathlib import Path
import re
import shlex
import shutil
import tempfile

from .common import clean_env, inventory, run_process


def pilot_documentation(doc):
    """Bounded lexical proxy, accepting shell line continuations and a known-ID paraphrase."""
    normalized = " ".join(re.sub(r"\\\s*\n", " ", doc.lower()).split())
    return ("--case alpha --case gamma" in normalized
            and "duplicat" in normalized and "order" in normalized and "all" in normalized
            and any(text in normalized for text in ["unknown", "invalid", "does not match a known"]))


def preserved_json(before, after):
    if not isinstance(after, dict) or set(after) != set(before):
        return False
    old, new = before["cases"], after.get("cases")
    if not isinstance(new, list) or len(new) != len(old) + 1:
        return False
    ids = [row.get("id") for row in new if isinstance(row, dict)]
    return (len(ids) == len(new) == len(set(ids)) and new[:-1] == old
            and ids[-1] == "observer-range-tail"
            and all(after[k] == v for k, v in before.items() if k != "cases"))


def score(case, suite_root, workspace, binary):
    original = Path(suite_root) / case["workspace"]
    workspace = Path(workspace)
    baseline = inventory(original)
    after = inventory(workspace)
    checks = {"supplied_files_preserved": all(after.get(p) == value for p, value in baseline.items()
                                              if p not in case["editable"])}
    logs = []
    with tempfile.TemporaryDirectory(prefix="spiral-independent-") as temp:
        isolated = Path(temp) / "workspace"
        shutil.copytree(original, isolated)
        for name in case["editable"]:
            source, target = workspace / name, isolated / name
            # Never follow a candidate symlink into the host or a reference file.
            parents = [workspace / p for p in Path(name).parents if p != Path(".")]
            if not source.is_file() or source.is_symlink() or any(p.is_symlink() for p in parents):
                checks["candidate_artifacts_present"] = False
                continue
            target.parent.mkdir(parents=True, exist_ok=True)
            shutil.copy2(source, target)
        checks.setdefault("candidate_artifacts_present", True)
        for index, check in enumerate(case["checks"]):
            code, log, expired = run_process(shlex.split(check["command"]), isolated,
                                           clean_env(binary), 45)
            key = f"independent_check_{index}"
            checks[key] = code == 0 and not expired
            if index == 0:
                counts = re.findall(r"Ran (\d+) tests? in", log)
                checks["original_test_count"] = bool(counts) and int(counts[-1]) >= case["minimum_tests"]
            logs.append({"command": check["command"], "exit_code": code,
                         "timed_out": expired, "output": log})
        try:
            if case["oracle"] == "large_file":
                expected = (original / "greet.py").read_bytes().replace(b"{name}?", b"{name}!")
                # Preservation is separate from behavior; do not accept deletion of the long prefix.
                actual = (isolated / "greet.py").read_bytes()
                checks["preserved_prefix"] = actual.splitlines(keepends=True)[:600] == expected.splitlines(keepends=True)[:600]
                exact_reference = actual == expected
            elif case["oracle"] == "pilot_cli":
                doc = (isolated / "docs/pilot.md").read_text().lower()
                checks["requested_documentation"] = pilot_documentation(doc)
                exact_reference = (isolated / "scripts/run_coupled_pilot.py").read_bytes() == (Path(suite_root) / case["reference"] / "scripts/run_coupled_pilot.py").read_bytes()
            else:
                before = json.loads((original / ".spiral-coder/tui_replay.json").read_text())
                candidate = json.loads((isolated / ".spiral-coder/tui_replay.json").read_text())
                checks["preserved_existing_json"] = preserved_json(before, candidate)
                doc = (isolated / "docs/replay.md").read_text().lower()
                checks["requested_documentation"] = all(x in doc for x in ["observer-range-tail", "src/tui/agent.rs", "1200", "1240", "advice"])
                exact_reference = None  # Valid new-case prose/JSON formatting need not match a golden patch.
        except (OSError, ValueError, KeyError, TypeError):
            checks["artifact_structure"] = False
            exact_reference = False
    return {"passed": all(checks.values()), "checks": checks, "logs": logs,
            "documentation_proxy_revision": "shell-whitespace-v2",
            "source_matches_reference": exact_reference}
