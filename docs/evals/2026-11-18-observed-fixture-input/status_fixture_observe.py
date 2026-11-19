"""Run one unchanged integration test and sample the files its CLI receives."""
import json
import os
from pathlib import Path
import re
import runpy
import shutil
import stat
import subprocess
import sys
import tempfile

CASE = "first_interruption_with_empty_ledger_preserves_known_totals"
TEST = Path("tests/coupled_status.rs")
MANIFEST = "iterations.json"
LEDGER = "state/tui-work.json"
MAX_JSON = 64 * 1024
ITERATION = re.compile(r"iteration-\d{2,4}", re.ASCII)


def read_json(file):
    try:
        metadata = file.lstat()
        if not stat.S_ISREG(metadata.st_mode) or metadata.st_size > MAX_JSON:
            return None, {"json": "unavailable"}
        with file.open("rb") as handle:
            data = handle.read(MAX_JSON + 1)
        if len(data) > MAX_JSON:
            return None, {"json": "unavailable"}
        value = json.loads(data)
    except FileNotFoundError:
        return None, {"json": "missing"}
    except json.JSONDecodeError as error:
        return None, {"json": "invalid", "line": error.lineno, "column": error.colno}
    except (OSError, ValueError, RecursionError):
        return None, {"json": "unavailable"}
    if not isinstance(value, dict):
        return None, {"json": "unsupported_shape"}
    return value, {"json": "valid"}


def number(value):
    return value if type(value) is int and 0 <= value < 2**53 else None


def iteration(value):
    return value if isinstance(value, str) and ITERATION.fullmatch(value) else None


def count_ledger(value, fields):
    requests = value.get("requests")
    if not isinstance(requests, list) or any(not isinstance(row, dict) for row in requests):
        return
    charges = [row.get("charge", "task") for row in requests]
    fields["wire_requests"] = len(charges)
    if all(charge in ("task", "transport") for charge in charges):
        fields["task_requests"] = charges.count("task")


def snapshot(root, stage):
    owner = Path(os.environ["SPIRAL_FIXTURE_ROOT"]).resolve()
    if root.is_symlink() or not root.is_dir() or not root.resolve().is_relative_to(owner):
        return
    value, fields = read_json(root / MANIFEST)
    if value is not None:
        rows = value.get("iterations")
        pending = value.get("interrupted_iteration")
        fields.update(
            finalized_rows=len(rows) if isinstance(rows, list) else None,
            interrupted_directory=iteration(pending.get("directory")) if isinstance(pending, dict) else None,
            task_requests=number(value.get("task_requests")),
            wire_requests=number(value.get("wire_requests")),
        )
    samples = [{"stage": stage, "file": MANIFEST, "fields": fields}]
    for entry in sorted(root.iterdir()):
        state = entry / "state"
        if not iteration(entry.name) or entry.is_symlink() or not entry.is_dir() or state.is_symlink():
            continue
        ledger, fields = read_json(entry / LEDGER)
        if ledger is not None:
            count_ledger(ledger, fields)
        samples.append({"stage": stage, "file": f"{entry.name}/{LEDGER}", "fields": fields})
        if len(samples) >= 6:
            break
    destination = Path(os.environ["SPIRAL_FIXTURE_SNAPSHOTS"]) / stage
    with tempfile.NamedTemporaryFile(mode="w", prefix=stage + "-", dir=destination, delete=False) as handle:
        json.dump(samples, handle)


def generate():
    # The copied generator runs with the same arguments and inputs as before.
    runpy.run_path("scripts/status_fixture_base.py", run_name="__main__")
    if len(sys.argv) == 3:
        try:
            snapshot(Path(sys.argv[1]), "generated")
        except OSError:
            pass


def comparisons(samples):
    by_file = {sample["file"]: sample["fields"] for sample in samples
               if sample["stage"] == "validator_input"}
    manifest = by_file[MANIFEST]
    rows = manifest.get("finalized_rows")
    pointer = manifest.get("interrupted_directory")
    if rows is None or pointer is None:
        return []
    expected_directory = f"iteration-{rows + 1:02}"
    next_ledger = by_file.get(f"{expected_directory}/{LEDGER}", {})
    selected_ledger = by_file.get(f"{pointer}/{LEDGER}", {})
    return [
        {"field": "manifest.interrupted_directory", "observed": pointer, "expected": expected_directory},
        {"field": f"{expected_directory}/{LEDGER} JSON", "observed": next_ledger.get("json", "missing"), "expected": "valid"},
        {"field": "selected pending ledger.wire_requests", "observed": selected_ledger.get("wire_requests"), "expected": 0},
    ]


def cargo(args, cwd, env):
    return subprocess.run(["cargo", *args, "--locked", "--offline"], cwd=cwd, env=env,
                          capture_output=True, timeout=240, check=True).stdout


def run():
    workspace = Path.cwd()
    cache = Path(os.environ.get("CARGO_TARGET_DIR", workspace / "target")).resolve()
    env = {k: os.environ[k] for k in (
        "PATH", "HOME", "TMPDIR", "SystemRoot", "RUSTUP_TOOLCHAIN",
        "CARGO_INCREMENTAL", "CARGO_PROFILE_DEV_DEBUG", "CARGO_PROFILE_TEST_DEBUG",
    ) if k in os.environ}
    env["CARGO_TARGET_DIR"] = str(cache)
    with tempfile.TemporaryDirectory(prefix="spiral-fixture-observe-") as temp:
        root = Path(temp)
        cargo(["build", "--bin", "spiral-coder"], workspace, env)
        name = "spiral-coder.exe" if os.name == "nt" else "spiral-coder"
        direct = root / name
        shutil.copy2(cache / "debug" / name, direct)
        scratch = root / "case"
        for name in ("src", "tests", "scripts"):
            (scratch / name).mkdir(parents=True)
        for file in (TEST, Path("Cargo.lock"), Path("scripts/native_work.json"), Path(__file__).resolve().relative_to(workspace)):
            shutil.copy2(workspace / file, scratch / file)
        (scratch / "Cargo.toml").write_bytes((workspace / "Cargo.toml").read_bytes()
            + b'\n[profile.fixture-observation]\ninherits = "dev"\ndebug = 0\nincremental = false\n')
        shutil.copyfile("scripts/status_fixture.py", scratch / "scripts/status_fixture_base.py")
        (scratch / "scripts/status_fixture.py").write_text("from status_fixture_observe import generate\ngenerate()\n")
        shutil.copyfile("scripts/status_fixture_forwarder.rs", scratch / "src/main.rs")
        artifact_output = cargo(["test", "--test", "coupled_status", "--no-run",
                                 "--profile", "fixture-observation", "--message-format=json"], scratch, env)
        artifacts = (json.loads(line) for line in artifact_output.splitlines())
        executable, = (row["executable"] for row in artifacts
                       if row.get("executable") and row["target"]["name"] == "coupled_status")
        storage = root / "snapshots"
        for stage in ("generated", "validator_input"):
            (storage / stage).mkdir(parents=True)
        owned = root / "test-fixtures"
        owned.mkdir()
        env.update(TMPDIR=str(owned), TMP=str(owned), TEMP=str(owned), SPIRAL_FIXTURE_ROOT=str(owned),
                   SPIRAL_FIXTURE_CLI=str(direct), SPIRAL_FIXTURE_PYTHON=sys.executable,
                   SPIRAL_FIXTURE_SNAPSHOTS=str(storage), PYTHONPYCACHEPREFIX=str(root / "pycache"))
        process = subprocess.run([executable, CASE, "--exact", "--nocapture"], cwd=scratch, env=env,
                                 capture_output=True, timeout=30)
        stdout = process.stdout.decode("utf-8", errors="replace")
        sys.stdout.write(stdout)
        sys.stderr.buffer.write(process.stderr)
        if "\nrunning 1 test\n" not in stdout:
            print("The observation runner requires exactly one selected integration test.", file=sys.stderr)
            return 2
        if process.returncode:
            samples = []
            for stage in ("generated", "validator_input"):
                files = list((storage / stage).iterdir())
                if len(files) == 1:
                    samples.extend(json.loads(files[0].read_bytes()))
            if any(sample["stage"] == "validator_input" for sample in samples):
                frame = {"version": 1, "case": CASE, "snapshots": samples, "comparisons": comparisons(samples)}
                print("Fixture observation v1: " + json.dumps(frame, separators=(",", ":")))
        return process.returncode


if __name__ == "__main__":
    if len(sys.argv) == 3 and sys.argv[1] == "--snapshot":
        snapshot(Path(sys.argv[2]), "validator_input")
    else:
        try:
            raise SystemExit(run())
        except subprocess.CalledProcessError as error:
            sys.stdout.buffer.write(error.stdout)
            sys.stderr.buffer.write(error.stderr)
            raise SystemExit(error.returncode)
