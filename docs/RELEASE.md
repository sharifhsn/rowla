# Release procedure

## Manual beta packages

The public beta uses manual downloads. It has no live update feed.
Beta 3 uses Developer ID and Apple notarization. Earlier betas use a development certificate without notarization.

1. Complete the checks in [CONTRIBUTING.md](../CONTRIBUTING.md).
2. Use a build number greater than all previous public builds.
3. Build the app with the chosen certificate.
4. Submit the Developer ID bundle for notarization and attach its ticket.
5. Do the native checks on that fixed bundle. Include the binary-floor check and signed shortcut imports.
6. Package the same bundle without another build or signature change.

```sh
TASKBAR_BUILD_NUMBER=INTEGER TASKBAR_SIGNING_IDENTITY=CERTIFICATE_SHA1 TASKBAR_SIGNING_MODE=developer-id scripts/build.sh
TASKBAR_NOTARY_PROFILE=KEYCHAIN_PROFILE scripts/notarize.sh dist/Rowla.app
scripts/package-beta.sh dist/Rowla.app dist/beta-OUTPUT 0.2.0-beta.NUMBER
```

Replace the example values. The package script accepts only Apple Silicon bundles without a live feed.
It creates the app archive and `SHA256SUMS`. The output directory must not exist.
The package script does not publish files.

Keep published archives and tags unchanged. Add a new release for changed app bytes.
Publish release files only after the release owner approves the specific release.

## Local signatures and permissions

A fresh checkout uses an ad-hoc signature. A changed ad-hoc build can need new macOS permission grants.
For repeated local builds, use a certificate from Keychain through `TASKBAR_SIGNING_IDENTITY` and `TASKBAR_SIGNING_MODE=local`.
Keep its private key in Keychain.

The build tools also accept a local JSON configuration at `~/Library/Application Support/Taskbar Rust/signing.json`:

```json
{
  "identity": "YOUR_40_CHARACTER_CERTIFICATE_SHA1",
  "mode": "local"
}
```

Use the same certificate for later builds. The tools reject an invalid configuration and do not silently change the signer.
macOS controls the permission grants. A stable certificate does not bypass its permission requirements.

`scripts/install.sh` installs a complete bundle. It does not compile or sign that bundle.
It rejects a certificate change by default when the destination has a certificate signature.
Quit the installed app before replacement.

## Optional automatic updates

The manual beta uses `scripts/notarize.sh`. The other release tools also prepare a signed Sparkle update feed.
The current beta has no live update feed.

| Tool | Action |
| --- | --- |
| `scripts/release.sh` | Build, notarize, and create an archive and signed appcast |
| `scripts/notarize.sh` | Submit a Developer ID bundle through an existing notarytool Keychain profile |
| `scripts/ci-release.sh` | Prepare the same artifacts on an isolated GitHub Actions runner |
| `scripts/publish-github.sh` | Publish a notarized archive and signed appcast after owner approval |

The manual **Prepare signed release** workflow needs a `release` environment with certificates and secrets.
It uses read-only repository access and does not publish.
An ordinary contributor needs none of these secrets to build the app.

Keep certificates, private keys, and passwords out of source control.
The release scripts use environment values and Keychain credentials.
Examine the scripts before you configure a release environment.
Do not enable automatic updates until a signed public appcast and its release archives exist.

Source publication uses the MIT license. Include the third-party notices with each app distribution.
