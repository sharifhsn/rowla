#!/usr/bin/env python3
"""Own disposable, non-editable AppKit windows; accept JSON on stdin.

Examples: {"op":"open","count":8}, {"op":"close","count":8},
{"op":"report"}, {"op":"quit"}. Never touches another app's windows.
"""
import json
from pathlib import Path
import plistlib
import subprocess
import sys
import tempfile

root = Path(__file__).resolve().parents[1]
with tempfile.TemporaryDirectory(prefix="taskbar-interaction-qa-") as temporary:
    app = Path(temporary) / "Taskbar Interaction QA.app"
    mac = app / "Contents/MacOS"
    mac.mkdir(parents=True)
    (app / "Contents/Info.plist").write_bytes(plistlib.dumps({
        "CFBundleIdentifier": "io.sharif.taskbarrust.interactionfixture",
        "CFBundleName": "Taskbar Interaction QA",
        "CFBundleExecutable": "interaction-provider",
        "CFBundlePackageType": "APPL",
        "NSPrincipalClass": "InteractionFixture",
    }))
    executable = mac / "interaction-provider"
    subprocess.run(["xcrun", "clang", "-fobjc-arc", "-framework", "AppKit",
                    str(root / "tests/fixtures/interaction-provider.m"),
                    "-o", str(executable)], check=True)
    subprocess.run(["codesign", "--force", "--sign", "-", str(app)], check=True,
                   stdout=subprocess.DEVNULL, stderr=subprocess.DEVNULL)
    fixture = subprocess.Popen([str(executable)], stdin=subprocess.PIPE,
                               stderr=subprocess.DEVNULL, text=True)
    print(json.dumps({"fixture_app": str(app), "fixture_pid": fixture.pid}), flush=True)
    try:
        for line in sys.stdin:
            command = json.loads(line)
            if command.get("op") not in {"open", "close", "minimize", "hide", "delay", "report", "quit"}:
                raise ValueError("Unknown fixture command")
            fixture.stdin.write(json.dumps(command) + "\n")
            fixture.stdin.flush()
            if command["op"] == "quit":
                fixture.wait(timeout=5)
                break
    finally:
        if fixture.poll() is None:
            fixture.terminate()
        fixture.wait(timeout=5)
