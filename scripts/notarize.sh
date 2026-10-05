#!/bin/bash
# Credentials stay in a notarytool Keychain profile, never source or arguments.
set -euo pipefail
app="${1:?Pass the signed .app bundle}"
profile="${TASKBAR_NOTARY_PROFILE:?Set TASKBAR_NOTARY_PROFILE to a notarytool Keychain profile}"
codesign --verify --deep --strict "$app"
details="$(codesign -d --verbose=4 "$app" 2>&1)"
[[ "$details" == *'Authority=Developer ID Application:'* && "$details" == *'flags=0x10000(runtime)'* ]] || { printf 'Notarization requires a hardened Developer ID build\n' >&2; exit 1; }
if /usr/sbin/lsof -t "$app/Contents/MacOS/taskbar-rs" >/dev/null 2>&1; then printf 'Stop this bundle before stapling its signature\n' >&2; exit 1; fi
stage="$(mktemp -d)"
trap 'rm -rf "$stage"' EXIT
ditto -c -k --keepParent "$app" "$stage/Taskbar-Rust.zip"
xcrun notarytool submit "$stage/Taskbar-Rust.zip" --keychain-profile "$profile" --wait
xcrun stapler staple "$app"
xcrun stapler validate "$app"
spctl --assess --type execute --verbose=2 "$app"
printf 'Notarized and assessed: %s\n' "$app"
