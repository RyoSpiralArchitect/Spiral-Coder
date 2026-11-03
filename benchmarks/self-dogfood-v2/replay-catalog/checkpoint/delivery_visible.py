"""Exercise the public npm alias's actual argument-routing function."""

import json
import subprocess
import unittest


class CatalogDelivery(unittest.TestCase):
    def test_spiral_preserves_the_catalog_command_and_arguments(self):
        argv = ["tui-replay-list", "--filter", "PREFS", "--max-cases", "1"]
        code = (
            "const {argumentsFor} = require('./npm/spiral-coder/lib/launcher.cjs');"
            "process.stdout.write(JSON.stringify(argumentsFor('spiral', "
            "JSON.parse(process.argv[1]), process.cwd())));"
        )
        actual = subprocess.check_output(
            ["node", "-e", code, json.dumps(argv)], text=True
        )
        self.assertEqual(
            json.loads(actual), argv, "installed alias diverted the listing to tui"
        )
