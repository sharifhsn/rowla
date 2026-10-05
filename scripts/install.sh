#!/bin/bash
# Promote already tested bytes. This script never compiles or re-signs them.
set -euo pipefail
project_dir="$(cd "$(dirname "$0")/.." && pwd)"
candidate="${1:-$project_dir/dist/Rowla.app}"
destination="${2:-$HOME/Applications/Rowla.app}"
[[ "$(uname -s)" == Darwin ]] || { printf 'Installation requires macOS.\n' >&2; exit 1; }
[[ "$destination" == /*.app ]] || { printf 'Destination must be an absolute .app path.\n' >&2; exit 1; }
verify() {
    [[ "$(/usr/libexec/PlistBuddy -c 'Print :CFBundleIdentifier' "$1/Contents/Info.plist")" == io.sharif.taskbarrust ]] || return 1
    [[ "$(/usr/libexec/PlistBuddy -c 'Print :CFBundleExecutable' "$1/Contents/Info.plist")" == taskbar-rs ]] || return 1
    [[ -x "$1/Contents/MacOS/taskbar-rs" ]] || return 1
    codesign --verify --deep --strict "$1"
}
verify "$candidate"
if [[ -e "$destination" ]]; then
    verify "$destination"
    details="$(codesign --display --verbose "$destination" 2>&1)"
    if [[ "$details" != *'Signature=adhoc'* && "${TASKBAR_ALLOW_SIGNER_CHANGE:-0}" != 1 ]]; then
        requirements="$(codesign --display --requirements - "$destination" 2>&1)"
        previous_requirement=""
        while IFS= read -r line; do
            if [[ "$line" == 'designated => '* ]]; then previous_requirement="${line#designated => }"; fi
        done <<< "$requirements"
        [[ -n "$previous_requirement" ]] || { printf 'Cannot read the installed signing identity.\n' >&2; exit 1; }
        if ! codesign --verify --strict -R "=$previous_requirement" "$candidate"; then
            printf 'The candidate changes the installed signing identity; existing macOS grants would be lost. Keep the configured signer, or explicitly set TASKBAR_ALLOW_SIGNER_CHANGE=1 for an intentional migration.\n' >&2
            exit 1
        fi
    fi
fi
running() { [[ -f "$destination/Contents/MacOS/taskbar-rs" ]] && /usr/sbin/lsof -t "$destination/Contents/MacOS/taskbar-rs" >/dev/null 2>&1; }
if running; then printf 'Quit Rowla before replacing its installed bundle.\n' >&2; exit 1; fi
parent_dir="$(dirname "$destination")"
mkdir -p "$parent_dir"
stage_dir="$(mktemp -d "$parent_dir/.taskbar-install.XXXXXX")"
cleanup() {
    # Preserve the original if rollback failed. Never delete the only backup.
    if [[ -d "$stage_dir/previous.app" ]]; then
        printf 'Previous app retained at: %s\n' "$stage_dir/previous.app" >&2
    else
        rm -rf "$stage_dir"
    fi
}
trap cleanup EXIT
ditto "$candidate" "$stage_dir/Rowla.app"
verify "$stage_dir/Rowla.app"
if running; then printf 'The destination app started during staging; quit it before installing.\n' >&2; exit 1; fi
if [[ -d "$destination" ]]; then mv "$destination" "$stage_dir/previous.app"; fi
if ! mv "$stage_dir/Rowla.app" "$destination"; then
    [[ ! -d "$stage_dir/previous.app" ]] || mv "$stage_dir/previous.app" "$destination"
    exit 1
fi
if ! verify "$destination"; then
    mv "$destination" "$stage_dir/failed.app"
    [[ ! -d "$stage_dir/previous.app" ]] || mv "$stage_dir/previous.app" "$destination"
    exit 1
fi
printf 'Installed tested bundle: %s\n' "$destination"
