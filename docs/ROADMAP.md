# Rowla priorities

This roadmap gives contribution priorities. It does not promise release dates.
The current beta supports Apple Silicon and macOS 26+. Intel builds pass CI, but the public download and manual device coverage remain limited.

## Near-term priorities

| Priority | Outcome | Useful contribution |
| --- | --- | --- |
| Reliable daily use | Clicks, hover close, and restore work across applications | Reproduction steps and fixture regressions for a specific failure |
| First-run trust | New users can install and understand both permissions | Independent installation results and clearer instructions |
| Measured resource use | Memory, queues, and capture rates stay bounded | Repeatable workloads with CPU, memory, and latency comparisons |
| Compatibility | Displays, Spaces, sleep, and minimized windows recover correctly | A device/application test matrix with exact versions |
| Discoverability | Users can see the workflow before installation | A short demo from synthetic fixture windows with no private desktop content |

Notarization and a future Homebrew cask need a stable distribution process and owner approval.
The [feature assessment](MACOS_FEATURE_RESEARCH.md) recommends macOS 15.2+ for the next public compatibility release.
macOS 14 remains a plausible technical target. Older support still needs a complete API audit and runtime checks.
Automatic updates need a signed public feed.
These are proposals rather than current capabilities.

## macOS API experiments

The [assessment](MACOS_FEATURE_RESEARCH.md#highest-value-experiments) gives evidence, introduction versions, and limits for these proposals:

1. Review cooperative app activation for the intermittent click failures.
2. Compare single-image capture against the current stream for latency and memory.
3. Add macOS 15.2 capture lifecycle feedback.
4. Use macOS 26 compact metrics where native controls need them.
5. Expose explicit Sort and taskbar visibility actions through Shortcuts and Spotlight.
6. Add optional macOS 27 capture-state diagnostics.

Preserve the bounded capture design. No performance improvement is established until measurements show it.

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
