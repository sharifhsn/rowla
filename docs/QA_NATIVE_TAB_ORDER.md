# Stable native tab order

Checks on October 8, 2026 used Apple Silicon and macOS 26.6.2 (25G83).
The bundle supports macOS 15.2 and later.

AppKit can change the visible window ID when the user selects another native tab.
Rowla previously removed the old ID and appended the new ID, which moved the tile.
Rowla now transfers the old slot before it removes absent IDs.
A shared native tab control ID, process ID, and application identity establish the relationship.
Ambiguous matches retain discovery order.

Both Space-order options preserve manual order.
Focused tabs remain visible while the screen window list catches up with Accessibility focus.
The change adds no resident cache, native request, or capture.

## Results

Rust passed 57 tests, with three benchmarks ignored.
Format, Clippy, compatibility, and 23 Python tests passed.
One optional certificate test was skipped.
The UI check passed 24 tab changes with unchanged tile and neighbor frames, 624 hit tests, and 606 feedback checks.

The native AppKit fixture passed four tab changes across different physical window IDs.
The tile stayed between two synthetic neighbors.
Utility activation, stale-target rejection, exact inactive-tab close, and Core Foundation release checks passed.
One earlier run lost foreground focus. A separate repeat passed the complete fixture check.
Direct Ghostty controls were not exercised.

The installed app restarted and matched the tested executable:

```text
43c18e9e849da6f61fbbc45021def9bb14cdbcd23a4852a79af9775e285281f2
```

Raw reports remain in the ignored `validation/native-tab-order/` directory.
Refer to `after-sequential.json` and `native-ui.log`.
