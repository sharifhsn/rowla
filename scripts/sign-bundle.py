#!/usr/bin/env python3
"""Reuse a configured signing identity; private keys stay in Keychain."""
import json
import os
from pathlib import Path
import re
import subprocess
import sys
from collections.abc import Mapping


def signing_args(config: Path, env: Mapping[str, str]) -> list[str]:
    identity = env.get("TASKBAR_SIGNING_IDENTITY")
    keychain = env.get("TASKBAR_SIGNING_KEYCHAIN")
    mode = env.get("TASKBAR_SIGNING_MODE", "developer-id")
    if identity is None and config.exists():
        settings = json.loads(config.read_text())
        if not isinstance(settings, dict):
            raise ValueError("Signing configuration must be an object")
        identity = settings.get("identity")
        mode = settings.get("mode")
        keychain = settings.get("keychain")
        if not isinstance(identity, str) or not re.fullmatch(r"[0-9a-fA-F]{40}", identity):
            raise ValueError("Signing configuration requires an exact certificate SHA-1")
        if mode not in {"local", "developer-id"}:
            raise ValueError("Signing configuration mode must be local or developer-id")
    if identity is None:
        identity = "-"
    args = ["--force", "--sign", identity]
    if identity != "-":
        if mode == "local":
            args.append("--timestamp=none")
        elif mode == "developer-id":
            args += ["--options", "runtime", "--timestamp"]
        else:
            raise ValueError("Unsupported signing mode")
        if keychain is not None:
            if not isinstance(keychain, str) or not Path(keychain).is_absolute() or not Path(keychain).is_file():
                raise ValueError("Signing keychain must be an existing absolute path")
            args += ["--keychain", keychain]
    return args


def main() -> None:
    if len(sys.argv) != 2:
        raise ValueError("Pass one staged .app bundle")
    app = Path(sys.argv[1])
    config = Path(os.environ.get("TASKBAR_SIGNING_CONFIG", str(
        Path.home() / "Library/Application Support/Taskbar Rust/signing.json")))
    args = signing_args(config, os.environ)
    framework = app / "Contents/Frameworks/Sparkle.framework"
    if framework.is_dir():
        base = framework / "Versions/B"
        for target in [base / "XPCServices/Downloader.xpc", base / "XPCServices/Installer.xpc",
                       base / "Autoupdate", base / "Updater.app", framework]:
            subprocess.run(["/usr/bin/codesign", *args, str(target)], check=True)
    subprocess.run(["/usr/bin/codesign", *args, "--identifier", "io.sharif.taskbarrust", str(app)], check=True)
    subprocess.run(["/usr/bin/codesign", "--verify", "--deep", "--strict", str(app)], check=True)


if __name__ == "__main__":
    try:
        main()
    except (OSError, ValueError, subprocess.CalledProcessError) as error:
        sys.exit(f"Signing failed: {error}")
