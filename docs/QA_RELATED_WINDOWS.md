# Related windows and native tabs

Checks on October 8, 2026 used Apple Silicon and macOS 26.6.2 (25G83).
The bundle supports macOS 15.2 and later.

Related utility windows and native tabs appear as compact bubbles on their owner tile.
An explicit native parent can group a standard child window.
Independent and ambiguous windows keep separate tiles.

Each tile shows at most three controls. A count control opens the full list.
Click selects the exact target. Hover Command-W, middle-click, and the context menu close that target.

Native tab actions use retained Accessibility identities.
Inactive tabs receive no background screenshots.
Browser document tabs need application-specific support.
Ghostty uses [AppKit native tabs](https://github.com/ghostty-org/ghostty/blob/main/macos/Sources/Features/Terminal/TerminalController.swift).

Selection recovery permits four attempts within one second.
A later user control or a switch to another app cancels recovery.
Inactive-tab close waits up to two seconds for the selected native identity.
The actual close press never repeats.
Metadata limits are 128 tabs per group, 4096 retained controls, and 256 characters per label.
The thumbnail cache remains limited to 16 MiB and 32 images.

## Repeat the fixture check

1. Build the app with `scripts/build.sh`.
2. Start `python3 scripts/live-qa-fixture.py`.
3. Record the `fixture_pid`.
4. Send `{"op":"tabs"}` and `{"op":"child"}`.
5. Run `dist/Rowla.app/Contents/MacOS/taskbar-rs --check-fixture-related PID`.
6. Confirm `passed:true`, four selected tabs, exact inactive-tab close, and `cf_live:0`.
7. Send `{"op":"quit"}`.

Keep the bundle fixed and the fixture in the foreground throughout the check.

## Results

The native fixture passed related-window controls, four tab switches, stale-target rejection, utility activation, and exact inactive-tab close.
Minimized close preserved the other window and rejected duplicate close requests.
The UI checks passed 624 hit tests and 606 feedback checks.
Overflow, narrow layouts, selected-tab feedback, and reuse of controls passed.

Rust passed 46 tests, with three benchmarks ignored.
Format, Clippy, compatibility, and 19 Python tests passed. One optional certificate test was skipped.

Two cycles opened and closed 32 fixture windows per cycle.
RSS started at 122.7 MiB, peaked at 133.0 MiB, and ended at 109.3 MiB.
Direct ChatGPT and Ghostty controls were not exercised.
Fixture checks cover their window mechanisms. Finite checks do not establish absence of all future memory leaks.
