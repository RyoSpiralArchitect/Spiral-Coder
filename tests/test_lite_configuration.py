"""Offline compatibility checks; importing the Lite runtime must not create directories."""
import importlib.util
import os
from pathlib import Path
import re
from types import SimpleNamespace
import unittest
from unittest.mock import Mock, patch


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

    def test_every_indexed_local_script_has_a_lite_asset_route(self):
        html = (lite.WEB_ROOT / 'index.html').read_text(encoding='utf-8')
        assets = re.findall(r'<script src="(/assets/[^"?]+)(?:\?[^" ]*)?"', html)
        self.assertIn('/assets/core/ui.js', assets)
        for asset in assets:
            handler = SimpleNamespace(path=asset, _serve_file=Mock(), _send_bytes=Mock())
            lite.LiteHandler.do_GET(handler)
            if asset.endswith('/governor_contract.js'):
                handler._send_bytes.assert_called_once()
            else:
                handler._serve_file.assert_called_once()
                file_path, content_type = handler._serve_file.call_args.args
                self.assertTrue(file_path.is_file(), asset)
                self.assertIn('javascript', content_type)


if __name__ == '__main__':
    unittest.main()
