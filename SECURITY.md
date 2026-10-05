# Security policy

## Supported releases

Security fixes target the latest public beta and current `main`.
Earlier beta downloads can lack later fixes. This volunteer project has no response-time or patch-time guarantee.

## Report a vulnerability

Use [GitHub's private vulnerability report](https://github.com/sharifhsn/rowla/security/advisories/new).
Describe the affected version, reproduction steps, impact, and a possible fix if available.
Use a small fixture that contains no credentials or personal window content.
Do not put security details in a public issue before the maintainer reviews the report.

Ordinary bugs and feature requests belong in [Issues](https://github.com/sharifhsn/rowla/issues/new/choose).
Permission problems and installation questions belong in [Discussions](https://github.com/sharifhsn/rowla/discussions).

## Trust boundaries

Rowla uses Accessibility for window controls and optional Screen Recording for previews.
The app keeps preview images in memory. Its default build has no live update feed.
A future distributor must configure the Sparkle feed and signature keys before automatic updates can operate.

The current beta uses a development certificate. Apple did not notarize it.
Download app bundles only from the official release page. Compare the archive with the published `SHA256SUMS` when necessary.
Keep Gatekeeper and SIP enabled.
