#!/bin/bash
# Build, notarize and sign an appcast locally. This never publishes files.
set -euo pipefail
project_dir="$(cd "$(dirname "$0")/.." && pwd)"
cd "$project_dir"
: "${TASKBAR_SIGNING_IDENTITY:?Set an existing Developer ID Application identity}"
: "${TASKBAR_BUILD_NUMBER:?Set a monotonically increasing integer build number}"
: "${TASKBAR_NOTARY_PROFILE:?Set a notarytool Keychain profile}"
: "${TASKBAR_RELEASE_BASE_URL:?Set the HTTPS directory that will host the feed and archive}"
[[ "$TASKBAR_SIGNING_IDENTITY" != - ]] || { printf 'Releases cannot be ad-hoc signed\n' >&2; exit 1; }
python3 - <<'PY'
import os, pathlib, urllib.parse, xml.etree.ElementTree as ET
base = urllib.parse.urlsplit(os.environ['TASKBAR_RELEASE_BASE_URL'])
if base.scheme != 'https' or not base.hostname or base.username or base.password or base.query or base.fragment:
    raise SystemExit('Release base URL must be a clean HTTPS directory without credentials')
build = os.environ['TASKBAR_BUILD_NUMBER']
if not build.isdigit():
    raise SystemExit('Release build number must be an integer')
prior = pathlib.Path(os.environ.get('TASKBAR_PREVIOUS_APPCAST', 'dist/appcast.xml'))
if prior.exists():
    versions = ET.parse(prior).findall('.//{http://www.andymatuschak.org/xml-namespaces/sparkle}version')
    if not versions or any(not (v.text or '').isdigit() for v in versions):
        raise SystemExit('Previous appcast has no valid integer build number')
    if int(build) <= max(int(v.text) for v in versions):
        raise SystemExit('Release build number must be greater than every previously shipped build')
if pathlib.Path(f'dist/Rowla-{build}.zip').exists():
    raise SystemExit('This release archive already exists; preserve it and use a new build number')
PY
account="${TASKBAR_SPARKLE_ACCOUNT:-io.sharif.taskbarrust}"
"$project_dir/scripts/fetch-sparkle.sh"
export TASKBAR_UPDATE_PUBLIC_KEY
TASKBAR_UPDATE_PUBLIC_KEY="$("$project_dir/vendor/bin/generate_keys" --account "$account" -p)"
export TASKBAR_UPDATE_FEED_URL="${TASKBAR_UPDATE_FEED_URL:-${TASKBAR_RELEASE_BASE_URL%/}/appcast.xml}"
# Validate configuration before compiling or making any submission.
validation="$(mktemp)"
trap 'rm -f "$validation"' EXIT
cp scripts/Info.plist "$validation"
python3 scripts/configure-updates.py "$validation"
if [[ $# -eq 0 ]]; then set -- --universal; fi
scripts/build.sh "$@"
app="$project_dir/dist/Rowla.app"
scripts/notarize.sh "$app"
archive="$project_dir/dist/Rowla-$TASKBAR_BUILD_NUMBER.zip"
ditto -c -k --keepParent "$app" "$archive"
signature="$(vendor/bin/sign_update --account "$account" -p "$archive")"
feed_stage="$(mktemp -d "$project_dir/dist/.feed.XXXXXX")"
trap 'rm -f "$validation"; rm -rf "$feed_stage"' EXIT
python3 scripts/create-appcast.py "$app" "$archive" "${TASKBAR_RELEASE_BASE_URL%/}/$(basename "$archive")" "$feed_stage/appcast.xml" "$signature"
vendor/bin/sign_update --account "$account" "$feed_stage/appcast.xml"
vendor/bin/sign_update --account "$account" --verify "$archive" "$signature"
vendor/bin/sign_update --account "$account" --verify "$feed_stage/appcast.xml"
mv "$feed_stage/appcast.xml" dist/appcast.xml
printf 'Prepared archive and signed appcast in dist/. Publish both only after release approval.\n'
