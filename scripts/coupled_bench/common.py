"""Filesystem, process and input contracts shared by the benchmark driver."""
import hashlib
import json
import os
from pathlib import Path
import re
import signal
import subprocess


PROVIDERS = {
    "gemini": ("GEMINI_API_KEY", "https://generativelanguage.googleapis.com/v1beta/openai"),
    "openai": ("OPENAI_API_KEY", "https://api.openai.com/v1"),
    "mistral": ("MISTRAL_API_KEY", "https://api.mistral.ai/v1"),
    "anthropic": ("ANTHROPIC_API_KEY", "https://api.anthropic.com/v1"),
}


def sha(path):
    digest = hashlib.sha256()
    with Path(path).open("rb") as stream:
        for block in iter(lambda: stream.read(1024 * 1024), b""):
            digest.update(block)
    return digest.hexdigest()


def read_json(path):
    return json.loads(Path(path).read_text())


def write_json(path, value):
    Path(path).write_text(json.dumps(value, indent=2, ensure_ascii=False) + "\n")


def relative_path(value):
    path = Path(value)
    if not value or path.is_absolute() or ".." in path.parts or ".git" in path.parts:
        raise ValueError(f"Expected a relative fixture path: {value!r}")
    return path


def inventory(root):
    """Include untracked, ignored, deleted, mode and symlink changes; exclude Git internals."""
    root = Path(root)
    rows = {}
    for path in sorted(root.rglob("*")):
        if ".git" in path.relative_to(root).parts:
            continue
        name = path.relative_to(root).as_posix()
        if path.is_symlink():
            rows[name] = {"symlink": os.readlink(path)}
        elif path.is_file():
            rows[name] = {"sha256": sha(path), "bytes": path.stat().st_size,
                          "mode": path.stat().st_mode & 0o777}
    return rows


def clean_env(binary=None):
    names = ["PATH", "HOME", "USER", "TMPDIR", "LANG", "SYSTEMROOT", "WINDIR",
             "SSL_CERT_FILE", "SSL_CERT_DIR"]
    env = {name: os.environ[name] for name in names if name in os.environ}
    env["PYTHONNOUSERSITE"] = "1"
    if binary:
        env["PATH"] = str(Path(binary).parent) + os.pathsep + env.get("PATH", "")
    return env


def run_process(command, cwd, env, timeout):
    try:
        process = subprocess.Popen(command, cwd=cwd, env=env, stdout=subprocess.PIPE,
                                   stderr=subprocess.STDOUT, text=True, errors="replace",
                                   start_new_session=True)
    except OSError as error:
        return -1, str(error), False
    expired = False
    try:
        log, _ = process.communicate(timeout=timeout)
    except subprocess.TimeoutExpired:
        expired = True
        try:
            if os.name == "posix":
                os.killpg(process.pid, signal.SIGTERM)
            else:
                process.terminate()
        except ProcessLookupError:
            pass
        try:
            log, _ = process.communicate(timeout=5)
        except subprocess.TimeoutExpired:
            if os.name == "posix":
                os.killpg(process.pid, signal.SIGKILL)
            else:
                process.kill()
            log, _ = process.communicate()
    return process.returncode, log, expired


def select(rows, requested):
    ids = [row["id"] for row in rows]
    if len(ids) != len(set(ids)) or not ids:
        raise ValueError("IDs must be nonempty and unique")
    if any(not re.fullmatch(r"[a-z0-9][a-z0-9-]*", name) for name in ids):
        raise ValueError("IDs must use lowercase letters, digits and hyphens")
    unknown = set(requested) - set(ids)
    if unknown:
        raise ValueError("Unknown IDs: " + ", ".join(sorted(unknown)))
    return [row for row in rows if not requested or row["id"] in requested]


def validate(suite, matrix):
    if suite.get("version") != 1 or matrix.get("version") != 1:
        raise ValueError("Only suite/matrix version 1 is supported")
    select(suite["cases"], [])
    select(matrix["profiles"], [])
    for case in suite["cases"]:
        if type(case["max_requests"]) is not int or not 1 <= case["max_requests"] <= 64:
            raise ValueError("max_requests must be 1..64")
        if case["oracle"] not in ("large_file", "pilot_cli", "replay_json"):
            raise ValueError("Unknown independent oracle")
        for value in [case["workspace"], case["reference"], case["prompt"], *case["editable"]]:
            relative_path(value)
        if not case["checks"] or not any(c["baseline_failure"] for c in case["checks"]):
            raise ValueError("At least one declared failing baseline is required")
    for profile in matrix["profiles"]:
        if profile["provider"] not in PROVIDERS or not profile.get("model"):
            raise ValueError("Unknown provider or empty model")
    for key, maximum in [("max_tokens", 65536), ("timeout_seconds", 600), ("wall_seconds", 3600)]:
        if type(matrix[key]) is not int or not 1 <= matrix[key] <= maximum:
            raise ValueError(f"Invalid {key}")
    if matrix.get("same_model_both_roles") is not True:
        raise ValueError("This benchmark requires the same model for Coder and Observer")
