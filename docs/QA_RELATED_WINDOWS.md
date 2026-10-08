# Related windows and native tab bubbles

Local checks on October 8, 2026 used Apple Silicon and macOS 26.6.2 (25G83).
The executable and bundle still require macOS 15.2.

## Behavior

Related utility windows appear as small bubbles on their main window button.
An explicit native parent can also group a standard child window.
Ambiguous relationships and independent main windows keep separate buttons.

The bar keeps its existing height and single-line titles.
Each button shows at most three bubble controls. A count bubble opens the remaining list.
Hover shows the full name. Click selects the exact window or native tab.
Hover Command-W, middle-click, and the context menu close that target without quitting its application.

Native tabs use retained Accessibility control identities, rather than title matching for actions.
Inactive native tabs show names. They receive no separate background screenshot.
Ghostty uses the supported AppKit tab mechanism, as shown in its [native controller source](https://github.com/ghostty-org/ghostty/blob/main/macos/Sources/Features/Terminal/TerminalController.swift).
Browser document tabs still need application-specific support.

## Activation and close safeguards

Utility panels cannot become main windows. Rowla raises them without repeated main-window activation.
Native tab selection raises the owner only when keyboard focus belongs outside that tab group.
An already active application does not need another activation request.

Selection recovery keeps one pending target, with four attempts and a one-second deadline.
A later user control or a switch to another application cancels an obsolete request.
Closing an inactive tab first selects it and verifies its native identity in the focused window.
The close wait has a two-second deadline. An actual close press never repeats.

## Repeat the native check

1. Build and sign the app with `scripts/build.sh`.
2. Start `python3 scripts/live-qa-fixture.py` in another terminal.
3. Record its `fixture_pid`.
4. Send these commands to the fixture:

```json
{"op":"tabs"}
{"op":"child"}
```

5. Run `dist/Rowla.app/Contents/MacOS/taskbar-rs --check-fixture-related PID` with that PID.
6. Check `passed:true`, four selected tabs, the auxiliary window, and the exact closed tab.
7. Check that `cf_live` is zero.
8. Send `{"op":"quit"}` to remove the fixture.

The checker controls only the disposable fixture provider.
Keep the signed bundle fixed during the check.
Changing the foreground application can cancel a request or invalidate a stable-focus assertion.

## Results and limits

The native related-window check passed grouping, four tab switches, stale-target rejection, utility activation, and inactive-tab close.
All tracked Core Foundation objects were released after the worker stopped.
The minimized-window close check preserved the other window, including three duplicate requests while restoration completed.

The native UI check passed 624 hit tests and 606 feedback checks.
It also checked overflow, narrow layouts, exact hover-close targets, pending tab feedback, and control reuse across repeated renders.
Existing single-line titles, cursor recovery, cached minimized previews, Sort, and pinned-icon behavior passed their UI checks.

Ordinary, hidden, and 80 ms delayed activation regression cases passed.
Other stable-focus checks were interrupted when another application took focus.
Those runs do not establish complete activation coverage under uninterrupted conditions.

The installed bar exposed ChatGPT's Computer Use and Controls windows as bubbles.
Direct ChatGPT and Ghostty interaction checks were blocked by the computer-use tool's application restrictions.
The native fixture checks cover their supported window and AppKit tab mechanisms.

The installed app was sampled 56 times during two cycles that opened and closed 32 fixture windows.
RSS started at 122.7 MiB, peaked at 133.0 MiB, and ended at 109.3 MiB.
This finite check found no accumulating growth across those cycles. It does not prove the absence of all future leaks.
Bubble controls add no background captures. The existing bitmap cache remains limited to 16 MiB and 32 images.
Native metadata is limited to 128 tabs per group and 4096 retained tab controls overall.
Tab labels are limited to 256 characters.

Rust checks passed 46 tests, with three opt-in benchmarks ignored.
Formatting and Clippy checks passed. Python checks passed 19 tests, with one opt-in signing check skipped.
Bundle compatibility checks passed. Existing signing identity checks passed during installation.

The installed executable matched the tested bundle:

```text
60c688b96ad0255927d888614b9bc8291d262f068c09fc91abac9e63e244f189
```
