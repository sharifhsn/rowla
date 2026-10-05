#!/bin/bash
# Run only on an isolated GitHub-hosted macOS runner with release secrets.
set -euo pipefail
project_dir="$(cd "$(dirname "$0")/.." && pwd)"
cd "$project_dir"
[[ "${GITHUB_ACTIONS:-}" == true ]] || { printf 'Use release.sh for local releases\n' >&2; exit 1; }
for name in TASKBAR_CERTIFICATE_BASE64 TASKBAR_CERTIFICATE_PASSWORD TASKBAR_NOTARY_KEY_BASE64 TASKBAR_NOTARY_KEY_ID TASKBAR_NOTARY_ISSUER_ID TASKBAR_SPARKLE_PRIVATE_KEY TASKBAR_SIGNING_IDENTITY TASKBAR_RELEASE_BASE_URL TASKBAR_BUILD_NUMBER; do
    [[ -n "${!name:-}" ]] || { printf 'Missing release setting: %s\n' "$name" >&2; exit 1; }
done
stage="$(mktemp -d)"
chmod 700 "$stage"
old_default="$(security default-keychain -d user | tr -d '"' | xargs)"
keychain="$stage/release.keychain-db"
cleanup() {
    security default-keychain -d user -s "$old_default" >/dev/null 2>&1 || true
    security delete-keychain "$keychain" >/dev/null 2>&1 || true
    rm -rf "$stage"
}
trap cleanup EXIT
export TASKBAR_CI_SECRET_DIR="$stage"
# Decode without echoing secrets or relying on shell expansion/command tracing.
python3 - <<'PY'
import base64, os, pathlib
folder = pathlib.Path(os.environ['TASKBAR_CI_SECRET_DIR'])
for name, filename in [('TASKBAR_CERTIFICATE_BASE64', 'certificate.p12'), ('TASKBAR_NOTARY_KEY_BASE64', 'notary.p8')]:
    (folder / filename).write_bytes(base64.b64decode(os.environ[name], validate=True))
(folder / 'sparkle.key').write_text(os.environ['TASKBAR_SPARKLE_PRIVATE_KEY'].strip())
for path in folder.iterdir(): path.chmod(0o600)
PY
keychain_password="$(openssl rand -base64 32)"
security create-keychain -p "$keychain_password" "$keychain"
security set-keychain-settings -lut 21600 "$keychain"
security unlock-keychain -p "$keychain_password" "$keychain"
security default-keychain -d user -s "$keychain"
security list-keychains -d user -s "$keychain" "$old_default"
security import "$stage/certificate.p12" -k "$keychain" -P "$TASKBAR_CERTIFICATE_PASSWORD" -T /usr/bin/codesign >/dev/null
security set-key-partition-list -S apple-tool:,apple:,codesign: -s -k "$keychain_password" "$keychain" >/dev/null
export TASKBAR_NOTARY_PROFILE=TaskbarReleaseCI
xcrun notarytool store-credentials "$TASKBAR_NOTARY_PROFILE" --key "$stage/notary.p8" --key-id "$TASKBAR_NOTARY_KEY_ID" --issuer "$TASKBAR_NOTARY_ISSUER_ID" >/dev/null
scripts/fetch-sparkle.sh
export TASKBAR_SPARKLE_ACCOUNT=TaskbarReleaseCI
vendor/bin/generate_keys --account "$TASKBAR_SPARKLE_ACCOUNT" -f "$stage/sparkle.key" >/dev/null
# Check the previously published version; do not treat a network failure as an
# empty feed. Only the first release may intentionally have no previous feed.
if [[ "${TASKBAR_FIRST_RELEASE:-false}" != true ]]; then
    : "${TASKBAR_UPDATE_FEED_URL:?Set the public feed URL}"
    curl --fail --location --proto '=https' --proto-redir '=https' --max-time 60 "$TASKBAR_UPDATE_FEED_URL" -o "$stage/previous-appcast.xml"
    vendor/bin/sign_update --account "$TASKBAR_SPARKLE_ACCOUNT" --verify "$stage/previous-appcast.xml"
    export TASKBAR_PREVIOUS_APPCAST="$stage/previous-appcast.xml"
fi
scripts/release.sh --universal
TASKBAR_TEST_BUNDLE="$project_dir/dist/Rowla.app" python3 -m unittest discover -s tests -v
