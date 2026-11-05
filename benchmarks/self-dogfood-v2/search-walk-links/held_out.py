import sys
import tempfile
import unittest
from pathlib import Path
sys.path.insert(0, str(Path.cwd()))
from scripts.search_walk_fixture import native_search

BINARY = Path(sys.argv.pop()).resolve()


class HeldOut(unittest.TestCase):
    def test_directory_aliases_do_not_add_rows(self):
        with tempfile.TemporaryDirectory() as tmp:
            root = Path(tmp) / "workspace"; root.mkdir()
            deep = root / "scope" / "deep"; deep.mkdir(parents=True)
            (deep / "real.rs").write_text("first\nNeedle local\n")
            external = Path(tmp) / "external"; external.mkdir()
            (external / "marker.rs").write_text("Needle external\n")
            (root / "scope" / "other").symlink_to(external, target_is_directory=True)
            (root / "scope" / "mirror").symlink_to(deep, target_is_directory=True)
            with native_search(BINARY, root) as search:
                out = search("needle", "scope", True)
            self.assertIn("scope/deep/real.rs:2: Needle local", out)
            self.assertNotIn("scope/other/", out, "discovered directory link contributed a row")
            self.assertNotIn("scope/mirror/", out)
            self.assertIn("1 match(es)", out)

    def test_nested_file_alias_and_dangling_link(self):
        with tempfile.TemporaryDirectory() as tmp:
            root = Path(tmp) / "workspace"; root.mkdir()
            nested = root / "nested"; nested.mkdir()
            outside = Path(tmp) / "elsewhere.md"; outside.write_text("needle boundary\n")
            (nested / "indirect.md").symlink_to(outside)
            (nested / "missing.md").symlink_to(root / "missing")
            with native_search(BINARY, root) as search:
                out = search("needle")
            self.assertNotIn("boundary", out, "discovered nested file link contributed a row")
            self.assertTrue(out.startswith("[search_files] No matches"), out)


unittest.main(verbosity=2)
