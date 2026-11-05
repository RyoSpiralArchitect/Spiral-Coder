"""Run the original pilot driver with this new frozen plan and a fresh output."""
import importlib.util
from pathlib import Path
import sys

HERE = Path(__file__).resolve().parent
SOURCE = HERE.parent.parent / "2026-11-03-coupled-iterations/automatic-pilot/live.py"


def run(binary, output):
    spec = importlib.util.spec_from_file_location("frozen_iteration_pilot", SOURCE)
    driver = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(driver)
    driver.HERE = HERE
    driver.run(binary.resolve(), output.resolve())


if __name__ == "__main__":
    run(*(Path(argument) for argument in sys.argv[1:]))
