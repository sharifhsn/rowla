# First-click activation

Checks on October 7, 2026 used Apple Silicon and macOS 26.6.2 (25G83).

A late Accessibility raise reply previously stopped activation after restore.
An 80 ms fixture delay reproduced the need for a second click.
Rowla now requests app activation first and confirms the focused window.

Recovery permits four attempts within one second.
A later user control or a switch to another app cancels recovery.
Close, minimize, hide, and quit never repeat.

## Repeat the fixture check

1. Build the app with `scripts/build.sh`.
2. Start `python3 scripts/live-qa-fixture.py`.
3. Record the `fixture_pid`.
4. Send these commands:

```json
{"op":"minimize","count":2}
{"op":"hide"}
{"op":"delay","count":2,"milliseconds":80}
{"op":"report"}
```

5. Wait for two minimized windows and `active:false`.
6. Run `dist/Rowla.app/Contents/MacOS/taskbar-rs --check-fixture-activation PID`.
7. Confirm `passed:true`, `latest_target_wins:true`, and `cf_live:0`.
8. Send `{"op":"quit"}` to close the fixture.

Keep the bundle fixed throughout the check.
Repeat with 250 ms delay and ordinary, hidden, minimized, and hidden minimized windows.

## Results

Seven fixture cases passed 28 activation commands and seven rapid-switch checks.
All tracked Core Foundation objects were released. Peak checker RSS was 29.6 MiB.
The installed app passed single clicks with 80 ms and 250 ms delays.
Its RSS ranged from 88 to 110 MiB.
Worker probes had zero failures and 3.94 ms p95 latency.

Minimized close removed only the target.

Rust passed 43 tests, with three benchmarks ignored.
Format, Clippy, compatibility, 19 Python tests, 624 UI hit tests, and 606 feedback checks passed.
One optional certificate test was skipped.
These fixture checks do not establish behavior for every third-party app.
