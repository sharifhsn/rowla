#!/bin/bash
set -euo pipefail
project_dir="$(cd "$(dirname "$0")/.." && pwd)"
cache="$project_dir/vendor"
version=2.10.0
archive="$cache/Sparkle-$version.tar.xz"
sha=c2bf58aa8387266ac179357b1415d6f2635f044da8be41042af32425dae6da0c
mkdir -p "$cache"
if [[ ! -f "$archive" ]]; then
    download="$(mktemp "$cache/.sparkle.XXXXXX")"
    trap 'rm -f "$download"' EXIT
    curl --fail --location --silent --show-error "https://github.com/sparkle-project/Sparkle/releases/download/$version/Sparkle-$version.tar.xz" -o "$download"
    [[ "$(shasum -a 256 "$download" | cut -d ' ' -f1)" == "$sha" ]] || { printf 'Sparkle checksum mismatch\n' >&2; exit 1; }
    mv "$download" "$archive"
fi
[[ "$(shasum -a 256 "$archive" | cut -d ' ' -f1)" == "$sha" ]] || { printf 'Sparkle checksum mismatch\n' >&2; exit 1; }
stage="$(mktemp -d "$cache/.extract.XXXXXX")"
trap 'rm -rf "$stage"' EXIT
tar -xf "$archive" -C "$stage"
codesign --verify --deep --strict "$stage/Sparkle.framework"
rm -rf "$cache/Sparkle.framework" "$cache/bin"
mv "$stage/Sparkle.framework" "$cache/"
mv "$stage/bin" "$cache/"
cp "$stage/LICENSE" "$cache/Sparkle-LICENSE"
printf 'Verified Sparkle %s\n' "$version"
