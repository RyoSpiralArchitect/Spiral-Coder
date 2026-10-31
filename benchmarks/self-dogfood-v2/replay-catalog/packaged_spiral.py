"""Run the package's actual 'spiral' bin entry with the candidate's native executable."""
import hashlib
import json
from pathlib import Path
import re
import shutil
import subprocess
import sys
import tempfile
import unittest

BINARY = Path(sys.argv.pop()).resolve()


class PackagedSpiral(unittest.TestCase):
    def test_public_entry_lists_cases(self):
        host = re.search(r"^host: (\S+)$", subprocess.check_output(["rustc", "-vV"], text=True), re.M)[1]
        with tempfile.TemporaryDirectory() as tmp:
            root = Path(tmp)
            artifact = root / "input" / host / BINARY.name
            artifact.parent.mkdir(parents=True)
            shutil.copyfile(BINARY, artifact)
            package = root / "package"
            subprocess.run(["node", "scripts/prepare-npm.mjs", "--input", root / "input",
                            "--out", package, "--local-target", host], check=True)
            manifest = json.loads((package / "native-manifest.json").read_text())
            self.assertEqual(manifest["binaries"][host]["sha256"], hashlib.sha256(BINARY.read_bytes()).hexdigest())
            entry = package / json.loads((package / "package.json").read_text())["bin"]["spiral"]
            project = root / "project"
            project.mkdir()
            spec = project / "spec.json"
            spec.write_text(json.dumps({"cases": [{"id": "from-package", "tags": ["alias"],
                "prompt": "", "observer_response": {}, "checks": [{"kind": "suggestion_parsed"}]}]}))
            content = spec.read_bytes()
            result = subprocess.run(["node", entry, "tui-replay-list", "--spec", "spec.json", "--filter", "ALIAS"],
                                    cwd=project, capture_output=True, text=True, timeout=20)
            if result.returncode and "spiral-coder tui " in result.stderr:
                self.fail("spiral routed replay listing to tui")
            self.assertEqual(result.returncode, 0, result.stderr)
            self.assertEqual(json.loads(result.stdout), {"version": 1, "total": 1,
                "cases": [{"id": "from-package", "tags": ["alias"]}]})
            self.assertEqual(list(project.iterdir()), [spec])
            self.assertEqual(spec.read_bytes(), content)


if __name__ == "__main__":
    unittest.main(verbosity=2)
