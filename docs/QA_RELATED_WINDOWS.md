# Related windows and native tab bubbles

Original checks: October 8, 2026, Apple Silicon, macOS 26.6.2 (25G83). Minimum supported version: macOS 15.2.

Related utility windows and native tabs appear as compact bubbles on their main window tile.
Independent windows and ambiguous relationships keep separate tiles.
Each tile shows at most three controls. An overflow control opens the full list.

Hover shows the name. Click selects the exact target.
Hover Command-W, middle-click, and the context menu close that target.

Tab actions use retained Accessibility identities. Close first confirms the selected tab and never repeats the actual close press.
Selection recovery has four attempts and a one-second deadline. Close waits at most two seconds.
A later control or a switch to another application cancels obsolete tab requests.

Inactive tabs receive no background capture. Browser document tabs need separate application support.

Repeat with a fixed signed bundle:

1. Run `scripts/build.sh`.
2. Start `python3 scripts/live-qa-fixture.py` in another terminal.
3. Record `fixture_pid`.
4. Send `{"op":"tabs"}` and `{"op":"child"}` to the fixture.
5. Run `dist/Rowla.app/Contents/MacOS/taskbar-rs --check-fixture-related PID` with the fixture PID.
6. Examine `passed:true`, `tabs_selected:4`, `tab_order_stable:true`, `closed_only_tab:true`, and `cf_live:0`.
7. Send `{"op":"quit"}` to the fixture.

The checker controls only disposable windows. Keep foreground focus stable and do not rebuild or sign the bundle during the check.

Window groups, four tab changes, stale-target rejection, utility activation, inactive-tab close, and tracked Core Foundation cleanup passed.
The UI check passed 624 hit tests and 606 feedback checks, plus overflow, narrow layouts, close targets, and control reuse.
Ordinary, hidden, and 80 ms delayed activation passed. Other activation cases lost foreground focus and remain unverified in those runs.
Computer-use restrictions blocked direct ChatGPT and Ghostty checks. The fixture covers their native window and AppKit tab mechanisms.

October 9, 2026: Incomplete Accessibility scans preserve cached window groups.
A 21-scan regression keeps tile and bubble positions fixed and reuses their controls.
An absent explicit parent cannot redirect its child to another window.

Two cycles each opened and closed 32 fixture windows. Across 56 samples, RSS started at 122.7 MiB, peaked at 133.0 MiB, and ended at 109.3 MiB.
This finite check does not establish long-term memory behavior. Installation kept the signature identity and matched the tested executable.

Native metadata permits 128 tabs per group, 4096 retained tab controls overall, and 256 characters per label.
The thumbnail cache keeps its 16 MiB and 32-image limits.
Rust passed 46 tests, with three ignored benchmarks. Python passed 19 tests, with one certificate test skipped.
Format, Clippy, compatibility, and installation signature checks passed.
