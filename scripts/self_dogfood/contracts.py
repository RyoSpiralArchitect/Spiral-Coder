"""Caller-owned task contracts and provenance; candidate output has no authority."""
from datetime import datetime, timezone
from pathlib import Path
import re
import shutil
import subprocess

from coupled_bench.common import PROVIDERS, clean_env, inventory, read_json, relative_path, sha, write_json


def now():
    return datetime.now(timezone.utc).isoformat()


def bounded_int(value, low, high, label):
    if type(value) is not int or not low <= value <= high:
        raise ValueError(f"{label} must be {low}..{high}")


def validate(spec):
    allowed = {"version", "id", "prompt", "editable", "protected", "required_changed_paths", "native_checks", "checks", "probes", "reference", "visible_fixtures", "roles", "max_requests", "coder_chunk_requests", "wall_seconds", "evaluation_wall_seconds", "timeout_seconds", "max_tokens", "observer_max_tokens", "temperature"}
    if set(spec) - allowed:
        raise ValueError("Unknown task contract fields")
    if spec.get("version") != 1:
        raise ValueError("Only self-dogfood spec version 1 is supported")
    if not re.fullmatch(r"[a-z0-9][a-z0-9-]{0,63}", spec.get("id", "")):
        raise ValueError("Invalid task id")
    for key in ["editable", "native_checks", "checks", "probes", "reference"]:
        if not isinstance(spec.get(key), list) or not spec[key]:
            raise ValueError(f"{key} must be a nonempty list")
    if len(spec["editable"]) > 16 or len(set(spec["editable"])) != len(spec["editable"]):
        raise ValueError("Use up to 16 unique editable paths")
    for name in [spec["prompt"], *spec["editable"], *spec.get("protected", [])]:
        relative_path(name)
    required = spec.get("required_changed_paths", [])
    if not isinstance(required, list) or any(not isinstance(name, str) for name in required):
        raise ValueError("required_changed_paths must be a list of exact editable paths")
    if len(required) != len(set(required)) or not set(required).issubset(spec["editable"]):
        raise ValueError("required_changed_paths must be a unique subset of editable")
    if set(spec["editable"]) & set(spec.get("protected", [])):
        raise ValueError("Editable and protected paths overlap")
    if any(not isinstance(c, str) or not c.strip() for c in spec["native_checks"]):
        raise ValueError("native_checks must contain exact nonempty commands")
    bounded_int(spec["max_requests"], 6, 64, "max_requests")
    bounded_int(spec["coder_chunk_requests"], 4, spec["max_requests"] - 2, "coder_chunk_requests")
    bounded_int(spec["wall_seconds"], 1, 3600, "wall_seconds")
    bounded_int(spec["evaluation_wall_seconds"], 1, 3600, "evaluation_wall_seconds")
    bounded_int(spec["timeout_seconds"], 1, 600, "timeout_seconds")
    bounded_int(spec["max_tokens"], 1, 65536, "max_tokens")
    if "observer_max_tokens" in spec:
        bounded_int(spec["observer_max_tokens"], 1, 65536, "observer_max_tokens")
    if type(spec.get("temperature")) not in (int, float) or not 0 <= spec["temperature"] <= 2:
        raise ValueError("temperature must be 0..2")
    if set(spec.get("roles", {})) != {"coder", "observer"}:
        raise ValueError("Declare both roles")
    for role in spec["roles"].values():
        if role.get("provider") not in PROVIDERS or not isinstance(role.get("model"), str) or not role["model"].strip():
            raise ValueError("Each role needs a supported provider and model")
        if role.get("key_env", PROVIDERS[role["provider"]][0]) != PROVIDERS[role["provider"]][0]:
            raise ValueError("Use the provider's standard credential environment name")
        if set(role) - {"provider", "model", "key_env"}:
            raise ValueError("Role overrides only accept provider/model/key_env; endpoints use fixed provider origins")
    if spec["roles"]["coder"]["provider"] == spec["roles"]["observer"]["provider"]:
        raise ValueError("This lane requires different provider families")
    ids = set()
    built = False
    for check in spec["checks"]:
        fields = {"id", "kind", "baseline", "argv", "timeout_seconds", "expected_test_count", "expected_baseline_failures", "baseline_failure_contains", "minimum_passed_tests"}
        if set(check) - fields:
            raise ValueError("Unknown check contract fields")
        if check["id"] in ids or not re.fullmatch(r"[a-z0-9][a-z0-9-]{0,63}", check["id"]):
            raise ValueError("Check IDs must be unique")
        ids.add(check["id"])
        if check["kind"] not in ("build", "regression", "improvement") or check["baseline"] not in ("pass", "fail"):
            raise ValueError("Invalid check kind or baseline expectation")
        if not isinstance(check["argv"], list) or not check["argv"] or any(not isinstance(x, str) or not x for x in check["argv"]):
            raise ValueError("Checks require explicit argv, never shell command text")
        if check["argv"][0] not in ("cargo", "python3", "node", "{target_dir}/debug/spiral-coder"):
            raise ValueError("Checks must use cargo, python3, node or the candidate debug binary")
        if check["kind"] == "build" and check["argv"][:2] != ["cargo", "build"]:
            raise ValueError("Build checks must invoke cargo build")
        if check["argv"][0] == "{target_dir}/debug/spiral-coder" and not built:
            raise ValueError("Candidate runtime checks must follow a build check")
        built = built or check["kind"] == "build"
        if "minimum_passed_tests" in check:
            bounded_int(check["minimum_passed_tests"], 1, 100000, "minimum_passed_tests")
        if "expected_baseline_failures" in check:
            bounded_int(check["expected_baseline_failures"], 1, check.get("expected_test_count", 0), "expected_baseline_failures")
        if "expected_test_count" in check:
            bounded_int(check["expected_test_count"], 1, 100000, "expected_test_count")
        if check["baseline"] == "fail" or "baseline_failure_contains" in check:
            markers = check.get("baseline_failure_contains")
            if not isinstance(markers, list) or not markers or any(not isinstance(x, str) or not x.strip() for x in markers):
                raise ValueError("baseline_failure_contains must be a nonempty list of nonblank diagnostic strings")
        bounded_int(check["timeout_seconds"], 1, spec["evaluation_wall_seconds"], "check timeout")
    if len(ids) > 8 or not {"build", "regression", "improvement"}.issubset(c["kind"] for c in spec["checks"]):
        raise ValueError("Declare build, regression and improvement checks (up to 8)")
    if not any(c["kind"] == "improvement" and c["baseline"] == "fail" for c in spec["checks"]):
        raise ValueError("A failing improvement baseline is required")
    for key in ("probes", "reference"):
        targets = set()
        for overlay in spec[key]:
            relative_path(overlay["source"]); relative_path(overlay["target"])
            if overlay["target"] in targets:
                raise ValueError("Overlay targets must be unique within each group")
            targets.add(overlay["target"])
            if key == "reference" and overlay["target"] not in spec["editable"]:
                raise ValueError("Reference repair must stay within editable paths")
            if overlay.get("mode", "replace") not in ("replace", "append"):
                raise ValueError("Unknown overlay mode")
    validate_visible_fixtures(spec)


def validate_visible_fixtures(spec):
    if "visible_fixtures" not in spec:
        return
    fixtures = spec["visible_fixtures"]
    if not isinstance(fixtures, list) or not 1 <= len(fixtures) <= 8:
        raise ValueError("visible_fixtures must be a list of 1..8 protected overlays")
    forbidden = {relative_path(p).as_posix() for p in spec["editable"]}
    forbidden.update(relative_path(o["target"]).as_posix() for key in ("probes", "reference") for o in spec[key])
    targets = set()
    for overlay in fixtures:
        if not isinstance(overlay, dict) or set(overlay) - {"source", "target", "mode"}:
            raise ValueError("Visible fixtures only accept source, target and mode")
        for key in ("source", "target"):
            value = overlay.get(key)
            if not isinstance(value, str) or relative_path(value).as_posix() != value or value == ".":
                raise ValueError("Visible fixture paths must be canonical relative file paths")
        target = overlay["target"]
        if target in targets or target in forbidden:
            raise ValueError("Visible fixture targets must be unique and disjoint from editable, probe and reference paths")
        targets.add(target)
        if overlay.get("mode", "replace") not in ("replace", "append"):
            raise ValueError("Unknown visible fixture overlay mode")


def git_commit(repo, revision):
    return subprocess.check_output(["git", "-C", str(repo), "rev-parse", "--verify", f"{revision}^{{commit}}"], text=True, timeout=30).strip()


def git_snapshot(repo, revision, destination):
    """Read the Git tree itself: archive export-ignore/export-subst may lose source."""
    repo = repo.resolve()
    commit = git_commit(repo, revision)
    listing = subprocess.check_output(["git", "-C", str(repo), "ls-tree", "-rz", "--full-tree", commit], timeout=30)
    rows = []
    total = 0
    for row in listing.split(b"\0"):
        if not row:
            continue
        meta, name = row.split(b"\t", 1)
        mode, kind, oid = meta.decode().split()
        path = relative_path(name.decode("utf-8"))
        if kind != "blob" or mode not in ("100644", "100755"):
            raise ValueError("Snapshot must not contain symlinks, submodules or special files")
        rows.append((mode, oid, path))
    destination.mkdir()
    process = subprocess.Popen(["git", "-C", str(repo), "cat-file", "--batch"], stdin=subprocess.PIPE, stdout=subprocess.PIPE)
    try:
        for mode, oid, path in rows:
            process.stdin.write((oid + "\n").encode()); process.stdin.flush()
            actual, kind, size = process.stdout.readline().decode().split()
            size = int(size); total += size
            if actual != oid or kind != "blob" or size > 32 * 1024 * 1024 or total > 256 * 1024 * 1024:
                raise ValueError("Unexpected Git object or repository exceeds snapshot bounds")
            body = process.stdout.read(size)
            if len(body) != size or process.stdout.read(1) != b"\n":
                raise ValueError("Incomplete Git blob")
            target = destination / path
            target.parent.mkdir(parents=True, exist_ok=True)
            target.write_bytes(body); target.chmod(int(mode, 8) & 0o777)
        process.stdin.close()
        if process.wait(timeout=30):
            raise ValueError("Git snapshot failed")
    finally:
        if process.poll() is None:
            process.kill(); process.wait(timeout=10)
        if not process.stdin.closed:
            process.stdin.close()
        process.stdout.close()
    return commit


def copy_inputs(spec_path, spec, target):
    target.mkdir()
    sources = {spec["prompt"], *(o["source"] for k in ("probes", "reference", "visible_fixtures") for o in spec.get(k, []))}
    if "spec.json" in sources:
        raise ValueError("spec.json is reserved for the frozen contract")
    for name in sources:
        path = spec_path.parent / name
        if not path.is_file() or path.is_symlink() or not path.resolve().is_relative_to(spec_path.parent.resolve()):
            raise ValueError(f"Input must be a local regular file: {name}")
        dst = target / name
        dst.parent.mkdir(parents=True, exist_ok=True)
        shutil.copy2(path, dst)
    write_json(target / "spec.json", spec)


def apply_overlays(workspace, inputs, overlays):
    for item in overlays:
        target = workspace / relative_path(item["target"])
        if not target.resolve().is_relative_to(workspace.resolve()) or target.is_symlink():
            raise ValueError("Overlay escaped verification workspace")
        target.parent.mkdir(parents=True, exist_ok=True)
        content = (inputs / relative_path(item["source"])).read_bytes()
        with target.open("ab" if item.get("mode") == "append" else "wb") as stream:
            stream.write(content)


def driver_inventory(root):
    return {p.relative_to(root).as_posix(): sha(p) for p in sorted(root.rglob("*.py")) if "__pycache__" not in p.parts}


def freeze_driver(out):
    scripts = Path(__file__).resolve().parent.parent
    target = out / "driver"
    target.mkdir()
    for module in ("self_dogfood", "coupled_bench"):
        shutil.copytree(scripts / module, target / module, ignore=shutil.ignore_patterns("__pycache__"))
    shutil.copy2(scripts / "run_self_dogfood.py", target / "run_self_dogfood.py")
    return driver_inventory(target)


def load_frozen(out, expected_digest):
    if not re.fullmatch(r"[0-9a-f]{64}", expected_digest or "") or sha(out / "predeclared.json") != expected_digest:
        raise ValueError("Predeclaration does not match the operator-pinned digest")
    declaration = read_json(out / "predeclared.json")
    if sha(out / "predeclared.json") != (out / "predeclared.sha256").read_text().strip():
        raise ValueError("Predeclaration changed")
    if sha(out / "layout.json") != declaration["layout_sha256"] or sha(out / "preflight.json") != declaration["preflight_sha256"]:
        raise ValueError("Frozen layout or preflight changed")
    layout = read_json(out / "layout.json")
    target = Path(layout["target_dir"])
    if target.resolve() != target or target.is_relative_to(Path(layout["source_repository"])) or any(target.is_relative_to(out / p) for p in ["baseline", "inputs", "driver", "runtime", "episode"]):
        raise ValueError("Build cache ownership changed")
    for name in ("inputs", "baseline"):
        if inventory(out / name) != declaration[f"{name}_inventory"]:
            raise ValueError(f"Frozen {name} changed")
    if sha(out / "episode/task.txt") != declaration["prompt_sha256"]:
        raise ValueError("Frozen task receipt changed")
    if sha(out / "runtime/spiral-coder") != declaration["binary_sha256"]:
        raise ValueError("Frozen runtime changed")
    if driver_inventory(out / "driver") != declaration["driver_sha256"]:
        raise ValueError("Frozen driver changed")
    here = Path(__file__).resolve().parent.parent
    # Compare only the modules that prepare captured, not unrelated local scripts.
    for name, digest in declaration["driver_sha256"].items():
        if not (here / name).is_file() or sha(here / name) != digest:
            raise ValueError("Use the frozen driver recorded by prepare")
    spec = read_json(out / "inputs/spec.json")
    validate(spec)
    return declaration, spec


def execution_env(out, target_dir, binary=None):
    env = clean_env(binary)
    env.update({"CARGO_TARGET_DIR": str(target_dir), "CARGO_INCREMENTAL": "0",
                "CARGO_PROFILE_DEV_DEBUG": "0", "CARGO_PROFILE_TEST_DEBUG": "0",
                "PYTHONPYCACHEPREFIX": str(out / "cache/python")})
    return env
