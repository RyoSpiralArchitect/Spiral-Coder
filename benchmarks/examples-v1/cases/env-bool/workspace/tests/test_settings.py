from pathlib import Path
import unittest

from settings import parse_bool


class BooleanSettings(unittest.TestCase):
    def test_true_tokens(self):
        for value in ("true", "1", "yes", "on"):
            with self.subTest(value=value):
                self.assertIs(parse_bool(value), True)

    def test_false_tokens(self):
        for value in ("false", "0", "no", "off"):
            with self.subTest(value=value):
                self.assertIs(parse_bool(value), False)

    def test_case_and_whitespace(self):
        self.assertIs(parse_bool(" \tYeS\n"), True)
        self.assertIs(parse_bool(" \tOFF\n"), False)

    def test_missing_uses_default(self):
        self.assertIs(parse_bool(None), False)
        self.assertIs(parse_bool(None, default=True), True)

    def test_empty_uses_default(self):
        self.assertIs(parse_bool(""), False)
        self.assertIs(parse_bool("", default=True), True)

    def test_whitespace_uses_default(self):
        self.assertIs(parse_bool(" \n\t"), False)
        self.assertIs(parse_bool(" \n\t", default=True), True)

    def test_explicit_false_overrides_default(self):
        self.assertIs(parse_bool("false", default=True), False)

    def test_unknown_strings_raise(self):
        for value in ("maybe", "enabled", "2", "truth", "false true"):
            with self.subTest(value=value), self.assertRaises(ValueError):
                parse_bool(value)

    def test_original_string_is_unchanged(self):
        value = "  false "
        parse_bool(value)
        self.assertEqual(value, "  false ")

    def test_documented_examples_and_check(self):
        doc = Path("docs/settings.md").read_text()
        for example in ('parse_bool("OFF") -> False', 'parse_bool("", default=True) -> True',
                        "ValueError", "python3 -B -S -m unittest discover -s tests -v"):
            self.assertIn(example, doc)


if __name__ == "__main__":
    unittest.main()
