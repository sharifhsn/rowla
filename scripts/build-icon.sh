#!/bin/bash
set -euo pipefail
project_dir="$(cd "$(dirname "$0")/.." && pwd)"
[[ $# == 1 ]] || { printf 'Usage: scripts/build-icon.sh OUTPUT.icns\n' >&2; exit 1; }
work_dir="$(mktemp -d "${TMPDIR:-/tmp}/taskbar-icon.XXXXXX")"
trap 'rm -rf "$work_dir"' EXIT
iconset="$work_dir/AppIcon.iconset"
mkdir -p "$iconset"
for entry in '16 16x16' '32 16x16@2x' '32 32x32' '64 32x32@2x' \
    '128 128x128' '256 128x128@2x' '256 256x256' '512 256x256@2x' \
    '512 512x512' '1024 512x512@2x'; do
    read -r size name <<< "$entry"
    sips --resampleHeightWidth "$size" "$size" "$project_dir/assets/AppIcon.png" \
        --out "$iconset/icon_$name.png" >/dev/null
done
iconutil --convert icns "$iconset" --output "$1"
