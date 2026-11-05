import sys
import tempfile
import unittest
from pathlib import Path
sys.path.insert(0, str(Path.cwd()))
from scripts.search_walk_fixture import native_search

BINARY = Path(sys.argv.pop()).resolve()


class Control(unittest.TestCase):
    def test_regular_directory_and_explicit_regular_file(self):
        with tempfile.TemporaryDirectory() as tmp:
            root = Path(tmp) / "work"; root.mkdir()
            sub = root / "sub"; sub.mkdir()
            (sub / "regular.rs").write_text("Item\nsecond Item\n")
            hidden = sub / ".cache"; hidden.mkdir()
            (hidden / "skip.rs").write_text("Item hidden\n")
            with native_search(BINARY, root) as search:
                directory = search("item", "sub", True)
                explicit = search("Item", "sub/regular.rs")
            for result in (directory, explicit):
                self.assertIn("2 match(es)", result)
                self.assertIn("sub/regular.rs:1: Item", result)
                self.assertIn("sub/regular.rs:2: second Item", result)
                self.assertNotIn("Item hidden", result)


unittest.main(verbosity=2)
