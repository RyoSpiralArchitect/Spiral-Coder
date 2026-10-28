import sys
from pathlib import Path
import unittest

sys.path.insert(0, str(Path(__file__).resolve().parents[1] / "scripts"))
import repo_map


class SymbolRanking(unittest.TestCase):
    def score(self, names):
        entry = {"path": "src/neutral.rs", "symbols": [
            {"name": name, "qualname": name, "kind": "function", "lineno": index + 1}
            for index, name in enumerate(names)]}
        query = "check dangerous command"
        return repo_map.score_file(entry, query, repo_map.normalize_terms(query))

    def test_whole_identifier_beats_several_partial_symbol_matches(self):
        complete = self.score(["check_dangerous_command"])
        partial = self.score(["check_command_selection", "check_command_schema",
                              "check_command_resolution"])
        self.assertGreater(complete[0], partial[0])

    def test_partial_name_does_not_get_whole_identifier_credit(self):
        result = self.score(["check_command_selection"])
        self.assertNotIn("symbol_query", result[3]["feature_scores"])


if __name__ == "__main__":
    unittest.main()
