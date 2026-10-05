"""Signing policy regressions; never create keys or change system trust."""
import importlib.util
import json
import os
from pathlib import Path
import subprocess
import sys
import tempfile
import unittest

ROOT = Path(__file__).resolve().parents[1]
spec = importlib.util.spec_from_file_location("taskbar_signing", ROOT / "scripts/sign-bundle.py")
assert spec and spec.loader
signing = importlib.util.module_from_spec(spec)
spec.loader.exec_module(signing)


class SigningSettingsTests(unittest.TestCase):
    def test_explicit_release_identity_overrides_local_preferences(self):
        with tempfile.TemporaryDirectory() as directory:
            config = Path(directory) / "signing.json"
            config.write_text("invalid local configuration")
            args = signing.signing_args(config, {"TASKBAR_SIGNING_IDENTITY": "Developer ID Application: Example"})
            self.assertIn("--timestamp", args)
            self.assertIn("runtime", args)

    def test_configured_signer_never_silently_falls_back_to_adhoc(self):
        with tempfile.TemporaryDirectory() as directory:
            config = Path(directory) / "signing.json"
            for contents in ['{}', '{"identity":"-","mode":"local"}', '{"identity":"' + 'A' * 40 + '","mode":"unknown"}']:
                config.write_text(contents)
                with self.assertRaises(ValueError):
                    signing.signing_args(config, {})
            config.write_text(json.dumps({"identity": "A" * 40, "mode": "local"}))
            self.assertEqual(signing.signing_args(config, {}), ["--force", "--sign", "A" * 40, "--timestamp=none"])
            self.assertEqual(signing.signing_args(config, {"TASKBAR_SIGNING_IDENTITY": "-"}), ["--force", "--sign", "-"])


@unittest.skipUnless(sys.platform == "darwin" and os.environ.get("TASKBAR_TEST_LOCAL_IDENTITY"), "Opt-in existing local signing identity required")
class StableIdentityInstallTests(unittest.TestCase):
    def test_changed_build_keeps_identity_and_adhoc_downgrade_is_rejected(self):
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            sys.path.insert(0, str(ROOT / "tests"))
            from test_install import InstallTests
            fixture = InstallTests()
            fixture.root = root
            source = fixture.bundle("first", "first build")
            newer = fixture.bundle("second", "second build")
            destination = root / "installed/Rowla.app"
            env = os.environ | {"TASKBAR_SIGNING_IDENTITY": os.environ["TASKBAR_TEST_LOCAL_IDENTITY"], "TASKBAR_SIGNING_MODE": "local"}
            for app in (source, newer):
                subprocess.run([str(ROOT / "scripts/sign-bundle.sh"), str(app)], env=env, check=True, capture_output=True)
                result = subprocess.run([str(ROOT / "scripts/install.sh"), str(app), str(destination)], capture_output=True, text=True)
                self.assertEqual(result.returncode, 0, result.stderr)
            before = fixture.digest(destination)
            adhoc = fixture.bundle("adhoc", "wrong signer")
            result = subprocess.run([str(ROOT / "scripts/install.sh"), str(adhoc), str(destination)], capture_output=True, text=True)
            self.assertNotEqual(result.returncode, 0)
            self.assertIn("signing identity", result.stderr)
            self.assertEqual(fixture.digest(destination), before)


if __name__ == "__main__":
    unittest.main()
