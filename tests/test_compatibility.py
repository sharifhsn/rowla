"""Bundle-floor regressions without native binaries or system changes."""
import importlib.util
from pathlib import Path
import plistlib
import tempfile
import unittest
from unittest.mock import patch

ROOT = Path(__file__).resolve().parents[1]
spec = importlib.util.spec_from_file_location("rowla_compatibility", ROOT / "scripts/check-compatibility.py")
assert spec and spec.loader
compatibility = importlib.util.module_from_spec(spec)
spec.loader.exec_module(compatibility)


class CompatibilityTests(unittest.TestCase):
    def setUp(self):
        self.directory = tempfile.TemporaryDirectory()
        self.addCleanup(self.directory.cleanup)
        self.app = Path(self.directory.name) / "Rowla.app"
        self.info = {"LSMinimumSystemVersion": "15.2", "CFBundleVersion": "2", "CFBundleShortVersionString": "0.2.0"}
        (self.app / "Contents/MacOS").mkdir(parents=True)
        (self.app / "Contents/Info.plist").write_bytes(plistlib.dumps(self.info))
        (self.app / "Contents/MacOS/taskbar-rs").write_bytes(b"\xcf\xfa\xed\xfe")
        self.floor = "15.2"
        self.architectures = "arm64"
        self.build_commands = None

    def output(self, args, **kwargs):
        if args[0] == "lipo":
            return self.architectures
        return self.build_commands or f"cmd LC_BUILD_VERSION\n minos {self.floor}\n sdk 27.0\n tool LD\n version 27037.1\n"

    def check(self):
        with patch.object(compatibility.subprocess, "check_output", side_effect=self.output):
            return compatibility.check(self.app)

    def test_linker_version_does_not_raise_the_os_floor(self):
        self.assertTrue(self.check()["passed"])

    def test_higher_floor_is_rejected(self):
        self.floor = "26.0"
        with self.assertRaisesRegex(ValueError, "Unsupported minimum OS"):
            self.check()

    def test_each_architecture_needs_a_floor(self):
        self.architectures = "arm64 x86_64"
        with self.assertRaisesRegex(ValueError, "Missing architecture floor"):
            self.check()

    def test_legacy_build_command_accepts_only_its_os_version(self):
        self.build_commands = "cmd LC_VERSION_MIN_MACOSX\n cmdsize 16\n version 15.2\n sdk 27.0\n"
        self.assertTrue(self.check()["passed"])

    def test_bundle_floor_must_match_the_binary(self):
        self.info["LSMinimumSystemVersion"] = "26.0"
        (self.app / "Contents/Info.plist").write_bytes(plistlib.dumps(self.info))
        with self.assertRaisesRegex(ValueError, "must declare macOS 15.2"):
            self.check()


if __name__ == "__main__":
    unittest.main()
