"""Offline compatibility checks; importing the Lite runtime must not create directories."""
import importlib.util
import os
from pathlib import Path
import unittest
from unittest.mock import patch


MODULE_PATH = Path(__file__).resolve().parents[1] / 'scripts' / 'serve_lite.py'
spec = importlib.util.spec_from_file_location('spiral_coder_lite_under_test', MODULE_PATH)
lite = importlib.util.module_from_spec(spec)
with patch.object(Path, 'mkdir') as mkdir:
    spec.loader.exec_module(lite)
    IMPORT_MKDIR_CALLS = mkdir.call_count


class LiteConfigurationTests(unittest.TestCase):
    def test_import_does_not_create_the_default_workspace(self):
        self.assertEqual(IMPORT_MKDIR_CALLS, 0)

    def test_canonical_env_wins_even_when_empty(self):
        with patch.dict(os.environ, {'SPIRAL_CODER_LANG': '', 'OBS_LANG': 'fr'}, clear=True):
            self.assertEqual(lite._env('SPIRAL_CODER_LANG', 'en'), '')

    def test_legacy_env_can_migrate_without_reconfiguring(self):
        with patch.dict(os.environ, {'OBS_REQUIRE_EDIT_APPROVAL': 'false'}, clear=True):
            self.assertEqual(lite._env('SPIRAL_CODER_REQUIRE_EDIT_APPROVAL'), 'false')
            self.assertEqual(lite._env('SPIRAL_CODER_LANG', 'ja'), 'ja')
            self.assertIsNone(lite._env('SPIRAL_CODER_API_KEY'))

    def test_other_provider_env_names_stay_unchanged(self):
        with patch.dict(os.environ, {'OPENAI_API_KEY': 'test-only'}, clear=True):
            self.assertEqual(lite._env('OPENAI_API_KEY'), 'test-only')


if __name__ == '__main__':
    unittest.main()
