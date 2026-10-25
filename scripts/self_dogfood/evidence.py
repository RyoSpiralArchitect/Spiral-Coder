"""Bind native evidence to one externally pinned, completed runner receipt."""
from pathlib import Path
import re

from coupled_bench.common import inventory, read_json, sha, write_json
from .contracts import now


def seal_run(out, declaration_digest):
    receipt = {"version": 1, "finished_at": now(), "predeclaration_sha256": declaration_digest,
               "episode_inventory": inventory(out / "episode")}
    write_json(out / "run-receipt.json", receipt)
    return sha(out / "run-receipt.json")


def bound_run(out, declaration_digest, run_digest):
    # Missing receipts still permit inspection of an interrupted durable ledger,
    # but can never qualify a candidate. A supplied incorrect pin is an error.
    if not run_digest:
        return False
    path = out / "run-receipt.json"
    if not re.fullmatch(r"[0-9a-f]{64}", run_digest) or sha(path) != run_digest:
        raise ValueError("Run receipt does not match the operator-pinned digest")
    receipt = read_json(path)
    if receipt["predeclaration_sha256"] != declaration_digest or inventory(out / "episode") != receipt["episode_inventory"]:
        raise ValueError("Episode artifacts changed after the pinned run receipt")
    return True
