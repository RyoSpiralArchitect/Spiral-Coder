#!/usr/bin/env python3
"""Prepare, run and independently score a bounded Coder + Observer matrix.

prepare makes no provider calls. run requires environment credentials and refuses
to restart an episode. summarize is offline and retains each previous assessment.
"""
import argparse
import json
from pathlib import Path

from coupled_bench import runner


def main(argv=None):
    parser = argparse.ArgumentParser(description=__doc__)
    commands = parser.add_subparsers(dest="command", required=True)
    default = Path(__file__).resolve().parents[1] / "benchmarks/coding-v1"
    prepare = commands.add_parser("prepare", help="Freeze a matrix and validate broken/reference fixtures offline")
    prepare.add_argument("--suite", type=Path, default=default / "suite.json")
    prepare.add_argument("--matrix", type=Path, default=default / "matrix.json")
    prepare.add_argument("--binary", type=Path, required=True)
    prepare.add_argument("--runtime-commit", required=True, help="Source commit used to build the binary; also retain its SHA-256")
    prepare.add_argument("--out", type=Path, required=True)
    run = commands.add_parser("run", help="Spend the predeclared shared request budget once per episode")
    run.add_argument("--out", type=Path, required=True)
    for command in [prepare, run]:
        command.add_argument("--case", action="append", default=[])
        command.add_argument("--profile", action="append", default=[])
    summarize = commands.add_parser("summarize", help="Score with original tests outside each candidate workspace")
    summarize.add_argument("--out", type=Path, required=True)
    args = parser.parse_args(argv)
    try:
        result = getattr(runner, args.command)(args)
    except (ValueError, OSError, KeyError) as error:
        parser.error(str(error))
    if result is not None:
        print(json.dumps(result, indent=2))


if __name__ == "__main__":
    main()
