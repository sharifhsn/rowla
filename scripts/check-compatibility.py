#!/usr/bin/env python3
"""Check the minimum OS and architecture of each bundled Mach-O executable."""
from pathlib import Path
import json
import plistlib
import re
import subprocess
import sys

def version(value: str) -> tuple[int, ...]:
    parts = tuple(int(part) for part in value.split("."))
    return parts + (0,) * (3 - len(parts))


def check(app: Path) -> dict:
    app_info = plistlib.loads((app / "Contents/Info.plist").read_bytes())
    extension = app / "Contents/Extensions/RowlaActions.appex"
    extension_info = plistlib.loads((extension / "Contents/Info.plist").read_bytes())
    for info in [app_info, extension_info]:
        if version(info["LSMinimumSystemVersion"]) != version("15.2"):
            raise ValueError("Both app bundles must declare macOS 15.2")
    for key in ["CFBundleVersion", "CFBundleShortVersionString"]:
        if app_info[key] != extension_info[key]:
            raise ValueError("App and extension versions differ")
    if not (extension / "Contents/Resources/Metadata.appintents").is_dir():
        raise ValueError("App Intents metadata is missing")
    binary = app / "Contents/MacOS/taskbar-rs"
    expected = set(subprocess.check_output(["lipo", "-archs", str(binary)], text=True).split())
    records = []
    magic = {b"\xcf\xfa\xed\xfe", b"\xce\xfa\xed\xfe", b"\xca\xfe\xba\xbe", b"\xca\xfe\xba\xbf"}
    for path in sorted(app.rglob("*")):
        if not path.is_file() or path.is_symlink():
            continue
        with path.open("rb") as file:
            if file.read(4) not in magic:
                continue
        architectures = set(subprocess.check_output(["lipo", "-archs", str(path)], text=True).split())
        if not expected.issubset(architectures):
            raise ValueError(f"Missing architecture in {path.relative_to(app)}")
        output = subprocess.check_output(["vtool", "-show-build", str(path)], text=True)
        floors = re.findall(r"^\s*minos\s+(\d+\.\d+(?:\.\d+)?)", output, re.MULTILINE)
        if not floors:
            floors = re.findall(r"cmd\s+LC_VERSION_MIN_MACOSX\s+cmdsize\s+\d+\s+version\s+(\d+\.\d+(?:\.\d+)?)", output)
        if not floors or any(version(value) > version("15.2") for value in floors):
            raise ValueError(f"Unsupported minimum OS in {path.relative_to(app)}: {floors}")
        if len(floors) != len(architectures):
            raise ValueError(f"Missing architecture floor in {path.relative_to(app)}")
        if path in [binary, extension / "Contents/MacOS/RowlaActions"] and any(
            version(value) != version("15.2") for value in floors
        ):
            raise ValueError(f"Rowla executable must target 15.2: {path.relative_to(app)}")
        records.append({"path": str(path.relative_to(app)), "architectures": sorted(architectures), "minimums": floors})
    if not records:
        raise ValueError("No Mach-O binaries in the bundle")
    metadata = json.loads((extension / "Contents/Resources/Metadata.appintents/extract.actionsdata").read_bytes())
    if set(metadata["actions"]) != {"SortWindowsIntent", "ShowTaskbarsIntent", "HideTaskbarsIntent"}:
        raise ValueError("The three system actions are missing from the metadata")
    return {"passed": True, "minimum_macos": "15.2", "architectures": sorted(expected), "binaries": records}


if __name__ == "__main__":
    try:
        print(json.dumps(check(Path(sys.argv[1])), indent=2))
    except (OSError, ValueError, KeyError, IndexError, subprocess.CalledProcessError) as error:
        sys.exit(f"Compatibility check failed: {error}")
