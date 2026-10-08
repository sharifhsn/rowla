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

    def test_report_and_child_exit_must_both_succeed(self):
        for completed, passed, exit_code, expected in [
            (True, False, 0, 1),
            (False, True, 0, 1),
            (True, True, 0, 0),
            (True, True, 1, 1),
        ]:
            with self.subTest(completed=completed, passed=passed, exit_code=exit_code):
                self.assertEqual(
                    self.check_report({"completed": completed, "passed": passed}, exit_code), expected
                )


if __name__ == "__main__":
    unittest.main()
