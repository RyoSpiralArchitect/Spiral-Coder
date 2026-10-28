from pathlib import Path
import unittest

from summary import summarize


class EventSummary(unittest.TestCase):
    def test_empty(self):
        self.assertEqual(summarize([]), {"info": 0, "warning": 0, "error": 0})

    def test_counts_and_extra_fields(self):
        self.assertEqual(summarize(['{"level":"info","message":"hello"}',
                                    '{"level":"info"}', '{"level":"error"}']),
                         {"info": 2, "warning": 0, "error": 1})

    def test_all_levels(self):
        self.assertEqual(summarize(['{"level":"info"}', '{"level":"warning"}', '{"level":"error"}']),
                         {"info": 1, "warning": 1, "error": 1})

    def test_blank_lines(self):
        self.assertEqual(summarize(["", " \t\n", '{"level":"warning"}', "\n"]),
                         {"info": 0, "warning": 1, "error": 0})

    def test_iterator(self):
        self.assertEqual(summarize(iter(['{"level":"info"}'])),
                         {"info": 1, "warning": 0, "error": 0})

    def test_invalid_json_has_physical_line(self):
        with self.assertRaisesRegex(ValueError, r"\bline 3\b"):
            summarize(["\n", '{"level":"info"}', "{"])

    def test_non_objects(self):
        for value in ("null", "[]", "42", '"text"'):
            with self.subTest(value=value), self.assertRaisesRegex(ValueError, r"\bline 1\b"):
                summarize([value])

    def test_missing_or_invalid_level(self):
        for value in ('{}', '{"level":"debug"}', '{"level":"INFO"}',
                      '{"level":null}', '{"level":1}', '{"level":[]}'):
            with self.subTest(value=value), self.assertRaisesRegex(ValueError, r"\bline 2\b"):
                summarize(["", value])

    def test_input_unchanged(self):
        lines = [' {"level":"info"} ']
        summarize(lines)
        self.assertEqual(lines, [' {"level":"info"} '])

    def test_documentation(self):
        doc = Path("docs/summary.md").read_text()
        for example in ('{"info": 0, "warning": 0, "error": 0}', "blank", "ValueError", "line 3",
                        "python3 -B -S -m unittest discover -s tests -v"):
            self.assertIn(example, doc)


if __name__ == "__main__":
    unittest.main()
