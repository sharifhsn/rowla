#!/bin/bash
# Explicit publication step. Run only after the release owner approves it.
set -euo pipefail
project_dir="$(cd "$(dirname "$0")/.." && pwd)"
cd "$project_dir"
repository="${1:?Pass OWNER/PUBLIC_DOWNLOAD_REPOSITORY}"
[[ "$repository" =~ ^[A-Za-z0-9_-]+/[A-Za-z0-9_.-]+$ ]] || { printf 'Invalid repository\n' >&2; exit 1; }
[[ "$(gh api "repos/$repository" --jq .private)" == false ]] || { printf 'Download repository must be public\n' >&2; exit 1; }
account="${TASKBAR_SPARKLE_ACCOUNT:-io.sharif.taskbarrust}"
app="$project_dir/dist/Rowla.app"
feed="$project_dir/dist/appcast.xml"
vendor/bin/sign_update --account "$account" --verify "$feed"
metadata="$(python3 - "$repository" "$app" "$feed" <<'PY'
import pathlib, plistlib, sys, xml.etree.ElementTree as ET
repo, app, feed = sys.argv[1], pathlib.Path(sys.argv[2]), pathlib.Path(sys.argv[3])
info = plistlib.loads((app / 'Contents/Info.plist').read_bytes())
item = ET.parse(feed).find('./channel/item')
build = item.findtext('{http://www.andymatuschak.org/xml-namespaces/sparkle}version', '')
if not build.isdigit() or build != info['CFBundleVersion']:
    raise SystemExit('App and feed build numbers must match')
archive = f'Rowla-{build}.zip'
enclosure = item.find('enclosure')
expected = f'https://github.com/{repo}/releases/download/build-{build}/{archive}'
if enclosure.get('url') != expected:
    raise SystemExit('Archive URL must refer to this repository and immutable release tag')
if info.get('SUFeedURL') != f'https://github.com/{repo}/releases/latest/download/appcast.xml':
    raise SystemExit('Embedded appcast URL must refer to this public download repository')
print(build)
print(enclosure.get('{http://www.andymatuschak.org/xml-namespaces/sparkle}edSignature', ''))
PY
)"
build="${metadata%%$'\n'*}"
signature="${metadata#*$'\n'}"
archive="$project_dir/dist/Rowla-$build.zip"
vendor/bin/sign_update --account "$account" --verify "$archive" "$signature"
codesign --verify --deep --strict "$app"
details="$(codesign -d --verbose=4 "$app" 2>&1)"
[[ "$details" == *'Authority=Developer ID Application:'* ]] || { printf 'Public downloads require Developer ID signing\n' >&2; exit 1; }
xcrun stapler validate "$app"
spctl --assess --type execute "$app"
if gh release view "build-$build" --repo "$repository" >/dev/null 2>&1; then
    printf 'Release tag already exists; do not overwrite a shipped build\n' >&2; exit 1
fi
# Upload both files in a draft. Publishing occurs only after both succeed.
gh release create "build-$build" "$archive" "$feed" --repo "$repository" --draft --title "Rowla build $build" --notes 'Native Rust macOS taskbar. Requires macOS 26 or later. Accessibility controls windows; Screen Recording supplies local previews.'
gh release edit "build-$build" --repo "$repository" --draft=false --latest
printf 'Published: https://github.com/%s/releases/tag/build-%s\n' "$repository" "$build"
