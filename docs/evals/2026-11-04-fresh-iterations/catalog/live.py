"""Run a fresh catalog chain with the externally pinned self-dogfood driver."""
import argparse
import hashlib
import json
from pathlib import Path
import sys
from types import SimpleNamespace
from unittest.mock import patch

HERE = Path(__file__).resolve().parent


def digest(path):
    return hashlib.sha256(path.read_bytes()).hexdigest()


def frozen_driver(out, plan):
    declaration = out / "predeclared.json"
    if digest(declaration) != plan["declaration_sha256"]:
        raise ValueError("The prepared declaration differs from the pinned plan")
    frozen = out / "driver"
    for name, expected in json.loads(declaration.read_bytes())["driver_sha256"].items():
        if digest(frozen / name) != expected:
            raise ValueError("The frozen runner changed")
    sys.path.insert(0, str(frozen))
    from self_dogfood import runner
    return runner


def run(out, plan_file):
    plan = json.loads(plan_file.read_bytes())
    if digest(Path(__file__)) != plan["runner_sha256"]:
        raise ValueError("The chain launcher changed")
    runner = frozen_driver(out, plan)
    original = runner.command_and_env

    def chain_command(root, spec):
        argv, env, keys = original(root, spec)
        if argv.count("coupled-run") != 1 or "--iterations" in argv or "--seed-session" in argv:
            raise ValueError("Unexpected original command shape")
        return [*argv, "--iterations", str(plan["iterations"])], env, keys

    # Keep the stock preflight, cache reset, live HTTP, execution, inventories and seal.
    # Only the native scheduler flag differs from the original single-round command.
    with patch.object(runner, "command_and_env", chain_command):
        result = runner.run(SimpleNamespace(out=out, declaration_sha256=plan["declaration_sha256"]))
    print(json.dumps(result, indent=2))


if __name__ == "__main__":
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("out", type=Path)
    parser.add_argument("plan", type=Path)
    args = parser.parse_args()
    run(args.out.resolve(), args.plan.resolve())
