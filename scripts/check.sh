#!/bin/bash
# Keep local and CI checks in one place.
set -euo pipefail
project_dir="$(cd "$(dirname "$0")/.." && pwd)"
cd "$project_dir"
[[ $# -le 1 ]] || { printf 'Usage: scripts/check.sh [--core]\n' >&2; exit 1; }
case "${1:-}" in
    --core) scope=--lib ;;
    '') scope=--all-targets ;;
    *) printf 'Usage: scripts/check.sh [--core]\n' >&2; exit 1 ;;
esac
cargo fmt --all --check
cargo test --locked "$scope" -j 2
cargo clippy --locked "$scope" -j 2 -- -D warnings
for script in scripts/*.sh; do bash -n "$script"; done
[[ "$scope" != --lib ]] || exit 0
scripts/build.sh
python3 scripts/check-compatibility.py dist/Rowla.app
dist/Rowla.app/Contents/MacOS/taskbar-rs --check-compatibility
python3 scripts/check-native-ui.py dist/Rowla.app
TASKBAR_TEST_BUNDLE="$PWD/dist/Rowla.app" python3 -m unittest discover -s tests -v
