#!/bin/bash
set -euo pipefail
project_dir="$(cd "$(dirname "$0")/.." && pwd)"
cd "$project_dir"
[[ "$(uname -s)" == Darwin ]] || { printf 'The app bundle requires macOS.\n' >&2; exit 1; }
install_after=false
target_triple=""
universal=false
while [[ $# -gt 0 ]]; do
    case "$1" in
        --install) install_after=true; shift ;;
        --universal) universal=true; shift ;;
        --target)
            [[ $# -gt 1 ]] || { printf 'Missing target triple.\n' >&2; exit 1; }
            target_triple="$2"; shift 2 ;;
        *) printf 'Usage: scripts/build.sh [--target TARGET | --universal] [--install]\n' >&2; exit 1 ;;
    esac
done
if $universal && [[ -n "$target_triple" ]]; then printf 'Choose --target or --universal\n' >&2; exit 1; fi
xcrun --show-sdk-path >/dev/null
python3 "$project_dir/scripts/check-notices.py"
"$project_dir/scripts/fetch-sparkle.sh"
mkdir -p "$project_dir/dist"
dist_dir="$project_dir/dist/Rowla.app"
if [[ -f "$dist_dir/Contents/MacOS/taskbar-rs" ]] && /usr/sbin/lsof -t "$dist_dir/Contents/MacOS/taskbar-rs" >/dev/null 2>&1; then
    printf 'Quit processes using the dist bundle before rebuilding it; changing its signature invalidates profiling and permissions.\n' >&2
    exit 1
fi
if $universal; then
    rustup target add aarch64-apple-darwin x86_64-apple-darwin
    for architecture in aarch64-apple-darwin x86_64-apple-darwin; do
        cargo build --release --locked -j "${TASKBAR_BUILD_JOBS:-2}" --target "$architecture"
    done
elif [[ -n "$target_triple" ]]; then
    cargo build --release --locked -j "${TASKBAR_BUILD_JOBS:-2}" --target "$target_triple"
else
    cargo build --release --locked -j "${TASKBAR_BUILD_JOBS:-2}"
fi
target_dir="$(cargo metadata --no-deps --format-version 1 | /usr/bin/plutil -extract target_directory raw -o - -)"
binary_dir="$target_dir/${target_triple:+$target_triple/}release"
stage_dir="$(mktemp -d "$project_dir/dist/.build.XXXXXX")"
trap 'rm -rf "$stage_dir"' EXIT
app_dir="$stage_dir/Rowla.app"
mkdir -p "$app_dir/Contents/MacOS" "$app_dir/Contents/Resources" "$app_dir/Contents/Frameworks"
ditto "$project_dir/vendor/Sparkle.framework" "$app_dir/Contents/Frameworks/Sparkle.framework"
cp "$project_dir/vendor/Sparkle-LICENSE" "$app_dir/Contents/Resources/Sparkle-LICENSE"
if $universal; then
    lipo -create "$target_dir/aarch64-apple-darwin/release/taskbar-rs" "$target_dir/x86_64-apple-darwin/release/taskbar-rs" -output "$app_dir/Contents/MacOS/taskbar-rs"
else
    cp "$binary_dir/taskbar-rs" "$app_dir/Contents/MacOS/taskbar-rs"
fi
cp scripts/Info.plist "$app_dir/Contents/Info.plist"
"$project_dir/scripts/build-icon.sh" "$app_dir/Contents/Resources/AppIcon.icns"
public_guide="${TASKBAR_PUBLIC_GUIDE:-$project_dir/USER_GUIDE.md}"
[[ -f "$public_guide" ]] || { printf 'Public user guide is missing.\n' >&2; exit 1; }
cp "$public_guide" "$app_dir/Contents/Resources/README.md"
cp LICENSE THIRD_PARTY_NOTICES.md "$app_dir/Contents/Resources/"
ditto "$project_dir/licenses" "$app_dir/Contents/Resources/licenses"
build_number="${TASKBAR_BUILD_NUMBER:-$(date +%s)}"
[[ "$build_number" =~ ^[0-9]+$ ]] || { printf 'TASKBAR_BUILD_NUMBER must be an integer.\n' >&2; exit 1; }
/usr/libexec/PlistBuddy -c "Set :CFBundleVersion $build_number" "$app_dir/Contents/Info.plist"
"$project_dir/scripts/build-intents.sh" "$app_dir"
python3 "$project_dir/scripts/configure-updates.py" "$app_dir/Contents/Info.plist"
"$project_dir/scripts/sign-bundle.sh" "$app_dir"
# Check again after compilation in case a profiling workload started meanwhile.
if [[ -f "$dist_dir/Contents/MacOS/taskbar-rs" ]] && /usr/sbin/lsof -t "$dist_dir/Contents/MacOS/taskbar-rs" >/dev/null 2>&1; then
    printf 'The dist bundle is running; stop it before replacing it.\n' >&2; exit 1
fi
if [[ -d "$dist_dir" ]]; then mv "$dist_dir" "$stage_dir/previous.app"; fi
if ! mv "$app_dir" "$dist_dir"; then
    [[ ! -d "$stage_dir/previous.app" ]] || mv "$stage_dir/previous.app" "$dist_dir"
    exit 1
fi
printf 'Built: %s\n' "$dist_dir"
if $install_after; then "$project_dir/scripts/install.sh" "$dist_dir"; fi
