"""Follow up on the inventory-only control's unexpected zero exit from --help."""
import hashlib
import json
import os
from pathlib import Path
import subprocess
import sys
import tempfile


def probe(binary):
    with tempfile.TemporaryDirectory() as tmp:
        def run(*args):
            return subprocess.run([binary, *args], cwd=tmp, env={"HOME": tmp, "PATH": os.defpath},
                                  capture_output=True, timeout=10)
        help_output = run("--help")
        direct = run("tui-replay-list", "--help")
        listing_args = ["--provider", "openai-compatible", "--base-url", "http://127.0.0.1:9/v1",
                        "tui-replay-list", "--filter", "PREFS", "--max-cases", "1"]
        listing = run(*listing_args)
    return {"binary_sha256": hashlib.sha256(binary.read_bytes()).hexdigest(),
            "parent_help_exit": help_output.returncode, "subcommand_help_exit": direct.returncode,
            "help_output_byte_identical": direct.stdout == help_output.stdout,
            "parent_help_sha256": hashlib.sha256(help_output.stdout).hexdigest(),
            "subcommand_help_sha256": hashlib.sha256(direct.stdout).hexdigest(),
            "listing_argv": listing_args, "listing_exit": listing.returncode,
            "listing_stdout_bytes": len(listing.stdout), "listing_first_error": listing.stderr.decode().splitlines()[:1]}


if __name__ == "__main__":
    print(json.dumps(probe(Path(sys.argv[1]).resolve()), indent=2))
