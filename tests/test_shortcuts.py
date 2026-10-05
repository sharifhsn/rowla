"""Keep bundled shortcuts limited to one explicit local Rowla command."""
from pathlib import Path
import os
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
                actions = data["WFWorkflowActions"]
                self.assertEqual(len(actions), 2)
                self.assertEqual([action["WFWorkflowActionIdentifier"] for action in actions],
                                 ["is.workflow.actions.url", "is.workflow.actions.openurl"])
                parameters = actions[0]["WFWorkflowActionParameters"]
                self.assertEqual(parameters["WFURLActionURL"], f"rowla://{command}")
                self.assertEqual(actions[1]["WFWorkflowActionParameters"]["WFInput"], {
                    "Value": {"Type": "ActionOutput", "OutputUUID": parameters["UUID"], "OutputName": "URL"},
                    "WFSerializationType": "WFTextTokenAttachment",
                })
                self.assertGreater(source.with_suffix(".shortcut").stat().st_size, source.stat().st_size)

    @unittest.skipUnless(os.environ.get("TASKBAR_TEST_BUNDLE"), "A built bundle is required")
    def test_bundle_contains_exactly_the_reviewed_signed_shortcuts(self):
        bundle = Path(os.environ["TASKBAR_TEST_BUNDLE"])
        source = ROOT / "shortcuts"
        resources = bundle / "Contents/Resources/shortcuts"
        expected = {path.name for path in source.glob("*.shortcut")}
        self.assertEqual({path.name for path in resources.iterdir()}, expected)
        for name in expected:
            self.assertEqual((resources / name).read_bytes(), (source / name).read_bytes())
        self.assertFalse((bundle / "Contents/Extensions").exists())


if __name__ == "__main__":
    unittest.main()
