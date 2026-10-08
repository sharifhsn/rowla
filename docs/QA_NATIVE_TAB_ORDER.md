# Stable order for native tabs

Original checks: October 8, 2026, Apple Silicon, macOS 26.6.2 (25G83). Minimum supported version: macOS 15.2.

AppKit can change the visible window ID when the user selects another native tab.
Rowla treated that ID as a new window and moved its tile to the end.
Rowla now transfers the old taskbar position before it removes absent IDs.
The windows must share a native tab control ID, process ID, and application identity.
Ambiguous matches keep normal discovery order. Titles and geometry do not establish the relationship.

The fix keeps manual order with either Space-order option and adds no cache, native control request, or capture.
It also keeps the focused native tab visible until the screen window list catches up.
The thumbnail cache keeps its 16 MiB and 32-image limits.

Results:

- Rust: 57 tests passed, three benchmarks ignored. Format and Clippy passed.
- Python: 23 tests passed, one certificate test skipped. Bundle compatibility passed.
- Native UI: 24 tab changes kept the tile and neighbor positions with both Space-order options.
- Full UI: 624 hit tests and 606 feedback checks passed.
- AppKit fixture: four physical window ID changes kept the tile between two synthetic neighbors.
- Utility activation, stale-target rejection, exact inactive-tab close, and tracked Core Foundation cleanup passed.

One run lost foreground focus during utility activation. A separate repeat passed the complete fixture check.
The checks controlled disposable AppKit windows. They did not directly control Ghostty.
The installed app matched the tested executable and kept its signature identity.

Repeat the [related-window fixture check](QA_RELATED_WINDOWS.md).
The original reports are `validation/native-tab-order/after-sequential.json` and `validation/native-tab-order/native-ui.log`.
