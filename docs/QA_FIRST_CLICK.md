# First-click activation regression

Local checks on October 7, 2026 used Apple Silicon and macOS 26.6.2 (25G83).

## Reproduction

The previous activation path restored and raised the window before it requested app activation.
Each Accessibility call has a 20 ms timeout.
A late raise reply stopped the command after the restore animation. The app did not become active until a second click.

The disposable AppKit fixture reproduced this failure with one 80 ms delay in `accessibilityPerformRaise`.
The first Taskbar click restored the window but produced no active or key-window event. The second click produced both events.
The previous native checker also returned `AXCannotComplete` (`-25204`).

## Change

Rowla now requests app activation before restore and raise.
The window worker confirms the actual frontmost app and focused window.
It can recover from a late reply without another click.

Recovery has four attempts and a one-second retry deadline. It keeps one record of window IDs and times.
A later user control replaces or cancels that record. A switch to a third app also cancels recovery.
Close, minimize, hide, and quit do not repeat. The preview cache remains limited to 16 MiB and 32 images.

## Repeat the native check

1. Build and sign the app with `scripts/build.sh`.
2. Start `python3 scripts/live-qa-fixture.py` in a second terminal.
3. Record its `fixture_pid`.
4. Send these JSON commands to the fixture:

```json
{"op":"minimize","count":2}
{"op":"hide"}
{"op":"delay","count":2,"milliseconds":80}
{"op":"report"}
```

5. Wait until the report shows two minimized windows and `active:false`.
6. Run `dist/Rowla.app/Contents/MacOS/taskbar-rs --check-fixture-activation PID` with the fixture PID.
7. Make sure that `passed` and `latest_target_wins` are true and `cf_live` is zero.
8. Send `{"op":"quit"}` to close only the fixture windows.

The checker sends one command per target through the production window worker.
It also sends two rapid target changes and waits one second for an unwanted later focus change.
It examines only the fixture provider. Normal app discovery keeps its full scope.

Use a fixed signed bundle throughout a native check. Do not rebuild or sign that bundle while it runs.
Repeat with a 250 ms delay, and with ordinary, hidden, minimized, and hidden minimized windows.
macOS restore animations still take their normal time. A second click is not necessary to complete activation.

## Results

The seven fixture cases passed all 28 individual commands and all seven rapid-switch checks.
Each checker released all tracked Core Foundation objects. Peak checker RSS was 29.6 MiB.
The installed app also passed single Taskbar clicks with the 80 ms and 250 ms delays.
Its final key-window event after rapid clicks belonged to the newest target.
Installed app RSS samples ranged from 88 to 110 MiB across these checks and normal hover after fixture cleanup.

Rust checks passed 43 tests, with three opt-in benchmarks ignored. Clippy and formatting checks passed.
Python checks passed 19 tests, with the opt-in signing test skipped. Bundle compatibility checks passed.
The native UI check passed all 624 hit tests and 606 active-feedback checks, including cursor recovery.
Minimized close removed only its target. Worker probes had no failures and a 3.94 ms p95 latency.

The installed executable matched the tested bundle:

```text
a188e1bb4ded949342afb3ed2435be48929195da000485c7b19775ec16585935
```

These local checks do not establish behavior for every third-party app or a long-term memory profile.
