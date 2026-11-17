"""Exercise cross-copy cache reuse with real offline Cargo, without provider calls."""
import json
import os
from pathlib import Path
import shutil
import subprocess
import sys
import tempfile
import unittest

sys.path.insert(0, str(Path(__file__).resolve().parents[1] / "scripts"))
from coupled_bench.common import sha
from self_dogfood.build_cache import reset_package_cache
from self_dogfood.contracts import execution_env
from self_dogfood.runner import cache_lock


@unittest.skipUnless(shutil.which("cargo") and os.name == "posix", "requires Cargo and POSIX cache locks")
class CargoCacheTests(unittest.TestCase):
    def test_native_copy_cannot_execute_verifier_tests_after_package_reset(self):
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary).resolve()
            candidate, verifier, dependency = (root / p for p in ("candidate", "verifier", "dependency"))
            for path in (candidate, dependency):
                (path / "src").mkdir(parents=True)
            (dependency / "Cargo.toml").write_text('[package]\nname="fixture-dependency"\nversion="0.1.0"\nedition="2021"\n')
            (dependency / "src/lib.rs").write_text("pub fn value() -> bool { false }\n")
            (candidate / "Cargo.toml").write_text('[package]\nname="spiral-coder"\nversion="0.1.0"\nedition="2021"\n[dependencies]\nfixture-dependency={path="../dependency"}\n')
            (candidate / "src/lib.rs").write_text('#[test] fn visible_failure() { assert!(fixture_dependency::value()); }\n')
            target = root / "build"
            env = execution_env(root, target)

            def cargo(workspace, *args):
                return subprocess.run(["cargo", *args], cwd=workspace, env=env,
                                      capture_output=True, text=True, timeout=60)

            def dependency_artifacts(output):
                artifacts = {}
                for line in output.splitlines():
                    if not line.startswith("{"):
                        continue
                    message = json.loads(line)
                    if (message.get("reason") != "compiler-artifact"
                            or message["target"]["name"] != "fixture_dependency"):
                        continue
                    for filename in message["filenames"]:
                        path = Path(filename)
                        if path.suffix == ".rlib":
                            artifacts[path] = (sha(path), path.stat().st_mtime_ns)
                return artifacts

            lock = cargo(candidate, "generate-lockfile", "--offline")
            self.assertEqual(lock.returncode, 0, lock.stderr)
            shutil.copytree(candidate, verifier)
            (verifier / "src/lib.rs").write_text('#[test] fn visible_failure() {}\n#[test] fn independent_probe_only() {}\n')
            test_args = ("test", "--locked", "--offline", "--lib", "--message-format=json")
            with cache_lock(target):
                built = cargo(verifier, *test_args)
                self.assertEqual(built.returncode, 0, built.stdout + built.stderr)
                dependencies = dependency_artifacts(built.stdout)
                self.assertTrue(dependencies)
                reset_package_cache(root, candidate, target, root / "native-cache-reset.json")
                native = cargo(candidate, *test_args)
                self.assertEqual(native.returncode, 101, native.stdout + native.stderr)
                self.assertIn("0 passed; 1 failed;", native.stdout)
                self.assertNotIn("independent_probe_only", native.stdout)
                self.assertEqual(dependencies, dependency_artifacts(native.stdout))


if __name__ == "__main__":
    unittest.main()
