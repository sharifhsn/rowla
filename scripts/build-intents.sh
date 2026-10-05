#!/bin/bash
set -euo pipefail
project_dir="$(cd "$(dirname "$0")/.." && pwd)"
app_dir="${1:?Pass the staged Rowla.app}"
extension_dir="$app_dir/Contents/Extensions/RowlaActions.appex"
resources="$extension_dir/Contents/Resources"
scratch="$(mktemp -d "$project_dir/dist/.intents.XXXXXX")"
trap 'rm -rf "$scratch"' EXIT
sdk="$(xcrun --show-sdk-path)"
developer_dir="$(xcode-select -p)"
xcode_version="$(xcodebuild -version | awk '/Build version/{print $3}')"
mkdir -p "$extension_dir/Contents/MacOS" "$resources"
cp "$project_dir/native/Info.plist" "$extension_dir/Contents/Info.plist"
python3 - "$app_dir" "$extension_dir" <<'PY'
from pathlib import Path
import plistlib, sys
app, extension = map(Path, sys.argv[1:])
app_info = plistlib.loads((app / 'Contents/Info.plist').read_bytes())
path = extension / 'Contents/Info.plist'
info = plistlib.loads(path.read_bytes())
for key in ['CFBundleShortVersionString', 'CFBundleVersion', 'LSMinimumSystemVersion']:
    info[key] = app_info[key]
path.write_bytes(plistlib.dumps(info))
PY
sources=("$project_dir/native/RowlaActions.swift" "$project_dir/native/RowlaActionsExtension.swift")
printf '%s\n' "${sources[@]}" > "$scratch/sources.txt"
architectures="$(lipo -archs "$app_dir/Contents/MacOS/taskbar-rs")"
binaries=()
metadata_arch=""
for architecture in $architectures; do
    case "$architecture" in arm64|x86_64) ;; *) printf 'Unsupported architecture: %s\n' "$architecture" >&2; exit 1 ;; esac
    [[ -n "$metadata_arch" ]] || metadata_arch="$architecture"
    xcrun swiftc -O -whole-module-optimization -parse-as-library -application-extension \
        -warnings-as-errors -sdk "$sdk" -target "$architecture-apple-macosx15.2" \
        -module-name RowlaActions \
        -Xfrontend -const-gather-protocols-list -Xfrontend "$project_dir/native/const-protocols.json" \
        -emit-const-values-path "$scratch/$architecture.swiftconstvalues" \
        "${sources[@]}" -o "$scratch/$architecture"
    binaries+=("$scratch/$architecture")
done
lipo -create "${binaries[@]}" -output "$extension_dir/Contents/MacOS/RowlaActions"
printf '%s\n' "$scratch/$metadata_arch.swiftconstvalues" > "$scratch/const-values.txt"
xcrun appintentsmetadataprocessor --output "$resources" \
    --toolchain-dir "$developer_dir/Toolchains/XcodeDefault.xctoolchain" \
    --module-name RowlaActions --sdk-root "$sdk" --xcode-version "$xcode_version" \
    --platform-family macOS --deployment-target 15.2 --target-triple "$metadata_arch-apple-macosx15.2" \
    --source-file-list "$scratch/sources.txt" --swift-const-vals-list "$scratch/const-values.txt"
[[ -d "$resources/Metadata.appintents" ]] || { printf 'App Intents metadata is missing.\n' >&2; exit 1; }
