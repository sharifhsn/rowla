# Rowla priorities

This roadmap gives contribution priorities. It does not promise release dates.
The current beta supports Apple Silicon and macOS 15.2+. Intel builds pass CI, but the public download and manual device coverage remain limited.

## Near-term priorities

| Priority | Outcome | Useful contribution |
| --- | --- | --- |
| Reliable daily use | Clicks, hover close, and restore work across applications | Reproduction steps and fixture regressions for a specific failure |
| First-run trust | New users can install and understand both permissions | Independent installation results and clearer instructions |
| Measured resource use | Memory, queues, and capture rates stay bounded | Repeatable workloads with CPU, memory, and latency comparisons |
| Compatibility | Displays, Spaces, sleep, and minimized windows recover correctly | A device/application test matrix with exact versions |
| Discoverability | Users can see the workflow before installation | A short demo from synthetic fixture windows with no private desktop content |

Notarization and a future Homebrew cask need a stable distribution process and owner approval.
The 0.2 compatibility release sets macOS 15.2 as the minimum.
macOS 14 remains a possible technical target, with a separate maintenance decision and device checks necessary.
Automatic updates need a signed public feed.

## Implemented macOS API additions

The [assessment](MACOS_FEATURE_RESEARCH.md#highest-value-experiments) gives the API evidence and introduction versions.
The 0.2 source includes cooperative activation, capture lifecycle callbacks, compact native controls, and optional capture-state diagnostics.
It also provides explicit Sort, Show, and Hide actions through a separate App Intents extension.
Snapshot capture is selectable in Preferences. Stream remains the default until broader measurements justify a change.

Useful next contributions include capture comparisons and permission-dependent checks on macOS 15.2 and Intel devices.
Keep the capture limits and report exact workloads, OS versions, and measurement tools.

## First contributions

Documentation fixes, reproduction reports, fixture improvements, and application compatibility results are valuable.
Choose a [good first issue](https://github.com/sharifhsn/rowla/labels/good%20first%20issue) or a [help wanted issue](https://github.com/sharifhsn/rowla/labels/help%20wanted).
If no task fits, describe your idea in [Discussions](https://github.com/sharifhsn/rowla/discussions).
For a larger feature, discuss the user problem and a small first change before implementation.

## Product direction

Keep each window easy to find and restore. Keep Sort predictable and explicit.
Prioritize correct window actions and measured resource use before more visual effects.
Keep the app free to use from this MIT-licensed source, with local preferences and previews.
See [the demand research](MARKET_RESEARCH.md) for evidence and open questions.
