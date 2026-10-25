#!/usr/bin/env python3
"""Frozen cross-family self-dogfood: prepare, one bounded run, independent assessment."""
import argparse
import json
from pathlib import Path
from self_dogfood import runner


def main(argv=None):
    parser = argparse.ArgumentParser(description=__doc__)
    sub = parser.add_subparsers(dest="command", required=True)
    p = sub.add_parser("prepare", help="Freeze repository, operator probes and reference; verify preflight without providers")
    p.add_argument("--repo", type=Path, required=True)
    p.add_argument("--baseline-ref", required=True)
    p.add_argument("--runtime-commit", required=True)
    p.add_argument("--binary", type=Path, required=True)
    p.add_argument("--spec", type=Path, required=True)
    p.add_argument("--target-dir", type=Path, help="External Cargo cache; exclusive across run/checks")
    for cmd in [p, sub.add_parser("run", help="Run once with only the two declared provider credentials"),
                sub.add_parser("assess", help="Evaluate allowed candidate edits against frozen checks")]:
        cmd.add_argument("--out", type=Path, required=True)
        if cmd is not p:
            cmd.add_argument("--declaration-sha256", required=True, help="Digest emitted by prepare, retained independently of the candidate/episode")
        if cmd.prog.endswith(" assess"):
            cmd.add_argument("--run-sha256", help="Digest emitted by run; omission permits interrupted accounting only, never qualification")
    args = parser.parse_args(argv)
    try:
        result = (runner.evaluate if args.command == "assess" else getattr(runner, args.command))(args)
    except (OSError, ValueError, KeyError, RuntimeError) as error:
        parser.error(str(error))
    print(json.dumps(result, indent=2))


if __name__ == "__main__":
    main()
