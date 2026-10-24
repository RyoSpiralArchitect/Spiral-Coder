import json
from pathlib import Path
import unittest

class ReplayContract(unittest.TestCase):
    def test_new_range_case(self):
        data = json.loads(Path('.spiral-coder/tui_replay.json').read_text())
        rows = [c for c in data['cases'] if c['id'] == 'observer-range-tail']
        self.assertEqual(len(rows), 1)
        case = rows[0]
        self.assertIn('1200', case['prompt'])
        suggestion = case['observer_response']['suggestions'][0]
        self.assertEqual(suggestion['kind'], 'read')
        self.assertEqual(suggestion['suggested_tool'], 'read_file')
        self.assertEqual(suggestion['suggested_args'], {'path':'src/tui/agent.rs','start_line':1200,'end_line':1240})
        required = [('suggestion_parsed', None), ('hint_queued', None), ('coder_system_contains', 'start_line=1200'), ('coder_system_contains', 'end_line=1240')]
        for kind, value in required:
            self.assertTrue(any(c['kind'] == kind and (value is None or c.get('value') == value) for c in case['checks']))

if __name__ == '__main__':
    unittest.main()
