"""Bounded subprocess trees and streamed, capped logs without shell evaluation."""
import os
from pathlib import Path
import signal
import subprocess
import threading
import time

MAX_LOG_BYTES = 8 * 1024 * 1024


def stop_tree(process):
    try:
        if os.name == "posix":
            os.killpg(process.pid, signal.SIGKILL)
        else:
            subprocess.run(["taskkill", "/PID", str(process.pid), "/T", "/F"],
                           stdout=subprocess.DEVNULL, stderr=subprocess.DEVNULL, timeout=10)
    except (ProcessLookupError, OSError, subprocess.TimeoutExpired):
        pass


def run_process(argv, cwd, env, timeout, log_path, secrets=()):
    started = time.monotonic()
    log_path = Path(log_path)
    log_path.parent.mkdir(parents=True, exist_ok=True)
    exceeded = threading.Event()
    try:
        process = subprocess.Popen(argv, cwd=cwd, env=env, stdin=subprocess.DEVNULL,
                                   stdout=subprocess.PIPE, stderr=subprocess.STDOUT,
                                   start_new_session=True)
    except OSError as error:
        log_path.write_text(str(error))
        return {"exit_code": -1, "timed_out": False, "output_limit": False, "wall_seconds": 0}
    secret_bytes = [s.encode() for s in secrets if s]
    redactions = [0]
    overlap = max([len(s) for s in secret_bytes] + [1]) - 1

    def collect():
        pending = b""
        written = 0
        with log_path.open("wb") as log:
            while True:
                chunk = process.stdout.read(65536)
                pending += chunk
                if chunk and len(pending) <= overlap:
                    continue
                # Keep enough trailing bytes to recognize a key split across reads.
                length = len(pending) if not chunk else len(pending) - overlap
                for secret in secret_bytes:
                    # Replace across the complete buffer before retaining its tail.
                    redactions[0] += pending.count(secret)
                    pending = pending.replace(secret, b"[REDACTED]")
                length = len(pending) if not chunk else max(0, len(pending) - overlap)
                emit, pending = pending[:length], pending[length:]
                room = MAX_LOG_BYTES - written
                log.write(emit[:room]); written += min(room, len(emit))
                if len(emit) > room:
                    exceeded.set(); stop_tree(process); break
                if not chunk:
                    break
        process.stdout.close()

    collector = threading.Thread(target=collect, daemon=True)
    collector.start()
    expired = False
    try:
        process.wait(timeout=timeout)
    except subprocess.TimeoutExpired:
        expired = True
    finally:
        # A shell can exit before its descendants; they belong to this bounded run.
        stop_tree(process)
        process.wait(timeout=10)
        collector.join(timeout=10)
    if collector.is_alive():
        raise RuntimeError("Subprocess log collector did not terminate")
    return {"exit_code": process.returncode, "timed_out": expired,
            "output_limit": exceeded.is_set(), "log_redactions": redactions[0], "wall_seconds": round(time.monotonic() - started, 3)}
