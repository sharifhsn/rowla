# First-click activation regression

Original checks: October 7, 2026, Apple Silicon, macOS 26.6.2 (25G83).

An 80 ms late Accessibility raise reply stopped activation after restore. A second click was necessary.
Rowla now requests app activation before restore and raise, then confirms the frontmost app and focused window.
Recovery keeps one target, with four attempts and a one-second deadline.
A later control or a switch to a third application cancels the old request.

Close, minimize, hide, and quit do not repeat. The thumbnail cache keeps its 16 MiB and 32-image limits.

Repeat with a fixed signed bundle:

1. Run `scripts/build.sh`.
2. Start `python3 scripts/live-qa-fixture.py` in a second terminal.
3. Record `fixture_pid`.
4. Send these commands to the fixture:

```json
{"op":"minimize","count":2}
{"op":"hide"}
{"op":"delay","count":2,"milliseconds":80}
{"op":"report"}
```

5. Wait for two minimized windows and `active:false`.
6. Run `dist/Rowla.app/Contents/MacOS/taskbar-rs --check-fixture-activation PID` with the fixture PID.
7. Examine `passed:true`, `latest_target_wins:true`, and `cf_live:0`.
8. Send `{"op":"quit"}` to the fixture.

Repeat with a 250 ms delay and ordinary, hidden, minimized, and hidden minimized windows.
The checker sends one command per target and two rapid target changes. It controls only disposable fixture windows.
Do not rebuild or sign the bundle during its native checks.

Seven cases passed 28 commands and seven rapid-switch checks, with zero tracked Core Foundation objects left.
The installed app passed the 80 ms and 250 ms delays. Its final focus belonged to the newest target.
Peak checker RSS was 29.6 MiB. Installed app samples ranged from 88 to 110 MiB.

Rust passed 43 tests, with three ignored benchmarks. Python passed 19 tests, with one certificate test skipped.
Format, Clippy, compatibility, 624 UI hit tests, and 606 feedback checks passed.
Minimized close removed only its target. Worker probes had no failures and a 3.94 ms p95 latency.
These fixture checks do not establish behavior for every application or long-term memory use.
