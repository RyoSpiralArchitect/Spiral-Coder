import tempfile
import unittest
from pathlib import Path
from scripts.search_walk_fixture import native_binary, native_search


class SearchWalk(unittest.TestCase):
    def test_discovered_link_is_not_a_workspace_file(self):
        with tempfile.TemporaryDirectory() as tmp:
            root = Path(tmp) / "work"
            root.mkdir()
            outside = Path(tmp) / "outside.txt"
            outside.write_text("probe_tag outside-only\n")
            (root / "inside.txt").write_text("probe_tag regular\n")
            (root / "alias.txt").symlink_to(outside)
            with native_search(native_binary(), root) as search:
                text = search("probe_tag")
            self.assertIn("inside.txt:1: probe_tag regular", text)
            self.assertNotIn("outside-only", text, "recursive search followed a discovered file link")
