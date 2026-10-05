#!/bin/bash
# Package an already tested public beta. This script does not publish or re-sign it.
set -euo pipefail
project_dir="$(cd "$(dirname "$0")/.." && pwd)"
candidate="${1:?Pass the tested Rowla.app bundle}"
output="${2:?Pass an empty output directory}"
version="${3:?Pass the beta version, for example 0.1.0-beta.1}"
[[ "$version" =~ ^[0-9]+\.[0-9]+\.[0-9]+-beta\.[0-9]+$ ]] || { printf 'Use a semantic beta version.\n' >&2; exit 1; }
[[ ! -e "$output" ]] || { printf 'Output already exists. Keep previous release files.\n' >&2; exit 1; }
codesign --verify --deep --strict "$candidate"
python3 - "$candidate" <<'PY'
import pathlib, plistlib, subprocess, sys
app = pathlib.Path(sys.argv[1])
info = plistlib.loads((app / 'Contents/Info.plist').read_bytes())
if (app.name != 'Rowla.app' or info['CFBundleName'] != 'Rowla'
        or info['CFBundleIdentifier'] != 'io.sharif.taskbarrust'
        or info['CFBundleExecutable'] != 'taskbar-rs'):
    raise SystemExit('The candidate must be the Rowla application')
if info.get('SUFeedURL') or info.get('SUEnableAutomaticChecks') or info.get('SUAutomaticallyUpdate'):
    raise SystemExit('This manual beta must not include a live update feed')
architectures = subprocess.check_output(['lipo', '-archs', str(app / 'Contents/MacOS/taskbar-rs')], text=True).split()
if architectures != ['arm64']:
    raise SystemExit('This beta release supports Apple Silicon only')
resources = app / 'Contents/Resources'
allowed = {'AppIcon.icns', 'README.md', 'LICENSE', 'THIRD_PARTY_NOTICES.md', 'Sparkle-LICENSE', 'licenses'}
if set(p.name for p in resources.iterdir()) != allowed:
    raise SystemExit('Public resources must contain only the user guide, icon, and license notices')
for name in ['README.md', 'LICENSE', 'THIRD_PARTY_NOTICES.md', 'Sparkle-LICENSE']:
    if not (resources / name).stat().st_size:
        raise SystemExit('A required public resource is empty')
PY
mkdir -p "$output"
archive="$output/Rowla-$version-macos-arm64.zip"
ditto -c -k --keepParent --norsrc "$candidate" "$archive"
python3 - "$archive" "$output" <<'PY'
import hashlib, pathlib, sys
archive, output = map(pathlib.Path, sys.argv[1:])
digest = hashlib.file_digest(archive.open('rb'), 'sha256').hexdigest()
(output / 'SHA256SUMS').write_text(f'{digest}  {archive.name}\n')
print(f'Prepared {archive.name}\nSHA256 {digest}')
PY
