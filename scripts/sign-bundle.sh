#!/bin/bash
# Keep this entry point for existing build/release callers.
set -euo pipefail
exec python3 "$(dirname "$0")/sign-bundle.py" "$@"
