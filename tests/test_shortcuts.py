"""Keep bundled shortcuts limited to one explicit local Rowla command."""
from pathlib import Path
import plistlib
import unittest

ROOT = Path(__file__).resolve().parents[1]


class ShortcutTests(unittest.TestCase):
    def test_shortcuts_only_open_the_three_exact_local_commands(self):
        for command, title in [("sort", "Sort Windows"), ("show", "Show Taskbars"), ("hide", "Hide Taskbars")]:
            with self.subTest(command=command):
                source = ROOT / "shortcuts" / f"Rowla {title}.plist"
                data = plistlib.loads(source.read_bytes())
                self.assertEqual(data["WFWorkflowName"], f"Rowla {title}")
                self.assertEqual(data["WFWorkflowActions"], [
                    {"WFWorkflowActionIdentifier": "is.workflow.actions.url",
                     "WFWorkflowActionParameters": {"WFURLActionURL": f"rowla://{command}"}},
                    {"WFWorkflowActionIdentifier": "is.workflow.actions.openurl",
                     "WFWorkflowActionParameters": {}},
                ])
                self.assertGreater(source.with_suffix(".shortcut").stat().st_size, source.stat().st_size)


if __name__ == "__main__":
    unittest.main()
