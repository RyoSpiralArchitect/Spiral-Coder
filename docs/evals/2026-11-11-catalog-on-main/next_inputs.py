"""Materialize the same catalog task with its two report literals made explicit."""
from pathlib import Path
import runpy
import sys


REPORT_COMMANDS = (
    "python3 -S -m unittest -v scripts.catalog_visible",
    "cargo test --locked --bin spiral-coder -q",
)


def require_report_commands(task):
    marker = "<task_requirements>"
    if task.count(marker) != 1:
        raise ValueError("Expected one task requirements block")
    first, second = REPORT_COMMANDS
    clause = f"Final report must include `{first}` and `{second}`.\n\n"
    return task.replace(marker, clause + marker, 1)


def materialize(out):
    previous = Path(__file__).resolve().parents[1] / "2026-11-04-fresh-iterations/catalog-final/next_inputs.py"
    runpy.run_path(str(previous))["materialize"](out)
    prompt = out / "prompt.txt"
    prompt.write_text(require_report_commands(prompt.read_text()))


if __name__ == "__main__":
    materialize(Path(sys.argv[1]).resolve())
