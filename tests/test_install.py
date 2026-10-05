"""Real codesign/install failure cases in isolated folders; no desktop interaction."""
import hashlib
import os
from pathlib import Path
import plistlib
import shutil
import subprocess
import sys
import tempfile
import unittest

ROOT = Path(__file__).resolve().parents[1]

@unittest.skipUnless(sys.platform == "darwin", "macOS packaging tools required")
class InstallTests(unittest.TestCase):
    def setUp(self):
        self.temp = tempfile.TemporaryDirectory(prefix="taskbar-install-test-")
        self.root = Path(self.temp.name)
        self.source = self.bundle("source", "guide one")
        self.destination = self.root / "apps" / "Rowla.app"

    def tearDown(self):
        self.temp.cleanup()

    def bundle(self, folder, text):
        app = self.root / folder / "Rowla.app"
        (app / "Contents/MacOS").mkdir(parents=True)
        (app / "Contents/Resources").mkdir()
        executable = app / "Contents/MacOS/taskbar-rs"
        shutil.copyfile("/bin/sleep", executable)
        executable.chmod(0o755)
        with (app / "Contents/Info.plist").open("wb") as f:
            plistlib.dump(dict(CFBundleIdentifier="io.sharif.taskbarrust", CFBundleExecutable="taskbar-rs", CFBundleName="Rowla", CFBundlePackageType="APPL", CFBundleVersion="1"), f)
        (app / "Contents/Resources/README.md").write_text(text)
        subprocess.run(["/usr/bin/codesign", "--force", "--sign", "-", str(app)], check=True, capture_output=True)
        return app

    def install(self, source=None, success=True):
        result = subprocess.run([str(ROOT / "scripts/install.sh"), str(source or self.source), str(self.destination)], capture_output=True, text=True)
        self.assertEqual(result.returncode == 0, success, result.stdout + result.stderr)
        return result

    def digest(self, app):
        result = hashlib.sha256()
        for path in sorted(app.rglob("*")):
            if path.is_file():
                result.update(str(path.relative_to(app)).encode())
                result.update(path.read_bytes())
        return result.hexdigest()

    def test_promotes_exact_bytes_and_retains_previous_app(self):
        self.install()
        original = self.digest(self.destination)
        newer = self.bundle("newer", "guide two")
        self.install(newer)
        self.assertEqual(self.digest(self.destination), self.digest(newer))
        backups = list(self.destination.parent.glob(".taskbar-install.*/previous.app"))
        self.assertEqual(len(backups), 1)
        self.assertEqual(self.digest(backups[0]), original)

    def test_tampered_signature_does_not_replace_existing_app(self):
        self.install()
        original = self.digest(self.destination)
        (self.source / "Contents/Resources/README.md").write_text("tampered")
        self.install(success=False)
        self.assertEqual(self.digest(self.destination), original)

    def test_running_destination_is_not_replaced(self):
        self.install()
        original = self.digest(self.destination)
        process = subprocess.Popen([str(self.destination / "Contents/MacOS/taskbar-rs"), "30"])
        try:
            result = self.install(self.bundle("newer", "guide two"), success=False)
            self.assertIn("Quit Rowla", result.stderr)
            self.assertEqual(self.digest(self.destination), original)
        finally:
            process.terminate()
            process.wait(timeout=5)

    def test_foreign_bundle_is_rejected_even_with_valid_signature(self):
        self.install()
        original = self.digest(self.destination)
        info = self.source / "Contents/Info.plist"
        with info.open("rb") as f:
            contents = plistlib.load(f)
        contents["CFBundleIdentifier"] = "test.foreign.publisher"
        with info.open("wb") as f:
            plistlib.dump(contents, f)
        subprocess.run(["/usr/bin/codesign", "--force", "--sign", "-", str(self.source)], check=True, capture_output=True)
        self.install(success=False)
        self.assertEqual(self.digest(self.destination), original)

    def fail_commit(self, also_fail_rollback=False):
        shims = self.root / "shims"
        shims.mkdir()
        shim = shims / "mv"
        shim.write_text('''#!/bin/bash
if [[ "$2" == "$TASKBAR_TEST_FAIL_DEST" ]]; then
    if [[ "$1" == */.taskbar-install.*/Rowla.app ]]; then exit 7; fi
    if [[ "$TASKBAR_TEST_FAIL_ROLLBACK" == yes && "$1" == */previous.app ]]; then exit 8; fi
fi
exec /bin/mv "$@"
''')
        shim.chmod(0o755)
        env = os.environ | {"PATH": str(shims) + ":" + os.environ["PATH"], "TASKBAR_TEST_FAIL_DEST": str(self.destination), "TASKBAR_TEST_FAIL_ROLLBACK": "yes" if also_fail_rollback else "no"}
        return subprocess.run([str(ROOT / "scripts/install.sh"), str(self.source), str(self.destination)], env=env, capture_output=True, text=True)

    def test_commit_failure_rolls_back_original_bytes(self):
        self.install()
        original = self.digest(self.destination)
        result = self.fail_commit()
        self.assertNotEqual(result.returncode, 0)
        self.assertEqual(self.digest(self.destination), original)

    def test_failed_rollback_preserves_only_original_backup(self):
        self.install()
        original = self.digest(self.destination)
        result = self.fail_commit(also_fail_rollback=True)
        self.assertNotEqual(result.returncode, 0)
        backups = list(self.destination.parent.glob(".taskbar-install.*/previous.app"))
        self.assertEqual(len(backups), 1)
        self.assertEqual(self.digest(backups[0]), original)

if __name__ == "__main__":
    unittest.main()
