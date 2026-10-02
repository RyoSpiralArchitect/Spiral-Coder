import importlib.util
from pathlib import Path
import tempfile
import unittest
from unittest.mock import patch

spec = importlib.util.spec_from_file_location(
    "migration", Path(__file__).resolve().parents[1] / "scripts/migrate_legacy_state.py"
)
migration = importlib.util.module_from_spec(spec)
spec.loader.exec_module(migration)


class MigrationTests(unittest.TestCase):
    def test_interrupted_copy_leaves_no_partial_destination_and_can_retry(self):
        def fail_copy(source, destination):
            destination.write(b"partial")
            raise OSError("simulated disk full")

        with tempfile.TemporaryDirectory() as tmp:
            root = Path(tmp)
            (root / ".obstral.md").write_text("complete original")
            with patch.object(migration.shutil, "copyfileobj", side_effect=fail_copy):
                with self.assertRaises(OSError):
                    migration.migrate(root, apply=True)
            self.assertFalse((root / ".spiral-coder.md").exists())
            self.assertEqual(list(root.glob(".spiral-coder-migrate-*")), [])
            self.assertEqual(migration.migrate(root, apply=True)[0][0], "copied")
            self.assertEqual((root / ".spiral-coder.md").read_text(), "complete original")

    def test_dry_run_then_copy_preserves_historical_bytes_and_originals(self):
        with tempfile.TemporaryDirectory() as tmp:
            root = Path(tmp)
            (root / ".obstral/nested").mkdir(parents=True)
            source = root / ".obstral/nested/session.json"
            payload = b'{"content":"read .obstral.md", "schema":1}\r\n'
            source.write_bytes(payload)
            target = root / ".spiral-coder/nested/session.json"
            self.assertEqual(migration.migrate(root)[0][0], "would-copy")
            self.assertFalse(target.exists())
            self.assertEqual(migration.migrate(root, apply=True)[0][0], "copied")
            self.assertEqual(target.read_bytes(), payload)
            self.assertEqual(source.read_bytes(), payload)
            self.assertEqual(migration.migrate(root, apply=True)[0][0], "keep-existing")

    def test_existing_destination_wins(self):
        with tempfile.TemporaryDirectory() as tmp:
            root = Path(tmp)
            (root / ".obstral.md").write_text("old")
            (root / ".spiral-coder.md").write_text("new")
            self.assertEqual(migration.migrate(root, apply=True)[0][0], "keep-existing")
            self.assertEqual((root / ".spiral-coder.md").read_text(), "new")

    def test_source_and_destination_symlinks_are_not_followed(self):
        for symlink_source in (True, False):
            with self.subTest(source=symlink_source), tempfile.TemporaryDirectory() as tmp:
                root = Path(tmp)
                outside = root / "outside"
                outside.mkdir()
                (outside / "prefs.json").write_text("outside")
                legacy = root / ".obstral"
                if symlink_source:
                    legacy.symlink_to(outside, target_is_directory=True)
                else:
                    legacy.mkdir()
                    (legacy / "prefs.json").write_text("inside")
                    (root / ".spiral-coder").symlink_to(outside, target_is_directory=True)
                self.assertEqual(migration.migrate(root, apply=True)[0][0], "skip-symlink")
                self.assertEqual((outside / "prefs.json").read_text(), "outside")

    def test_blocking_destination_parent_is_preserved(self):
        with tempfile.TemporaryDirectory() as tmp:
            root = Path(tmp)
            (root / ".obstral").mkdir()
            (root / ".obstral/prefs.json").write_text("old")
            (root / ".spiral-coder").write_text("keep")
            self.assertEqual(migration.migrate(root, apply=True)[0][0], "keep-existing-parent")
            self.assertEqual((root / ".spiral-coder").read_text(), "keep")


if __name__ == "__main__":
    unittest.main()
