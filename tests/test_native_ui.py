"""A child exit code must not conceal a failed native UI report."""
from pathlib import Path
import json
import subprocess
import sys
import tempfile
import unittest

SCRIPT = Path(__file__).resolve().parents[1] / "scripts/check-native-ui.py"


class NativeUiReportTests(unittest.TestCase):
    def check_report(self, report, exit_code=0):
        with tempfile.TemporaryDirectory() as directory:
            bundle = Path(directory) / "Fixture.app"
            binary = bundle / "Contents/MacOS/taskbar-rs"
            binary.parent.mkdir(parents=True)
            binary.write_text(
                f"#!{sys.executable}\nprint({json.dumps(report)!r})\nraise SystemExit({exit_code})\n"
            )
            binary.chmod(0o755)
            return subprocess.run(
                [sys.executable, str(SCRIPT), str(bundle)],
                capture_output=True, text=True, timeout=10,
            ).returncode

    def test_failed_report_is_not_a_pass(self):
        self.assertEqual(self.check_report({"completed": True, "passed": False}), 1)

    def test_incomplete_report_is_not_a_pass(self):
        self.assertEqual(self.check_report({"completed": False, "passed": True}), 1)

    def test_successful_report_and_exit_code_pass(self):
        self.assertEqual(self.check_report({"completed": True, "passed": True}), 0)

    def test_child_failure_remains_a_failure(self):
        self.assertEqual(self.check_report({"completed": True, "passed": True}, 1), 1)


if __name__ == "__main__":
    unittest.main()
