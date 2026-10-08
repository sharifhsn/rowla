# Stable order for native tabs

Local checks on October 8, 2026 used Apple Silicon and macOS 26.6.2 (25G83).
The bundle still supports macOS 15.2 and later.

## Cause and correction

AppKit can change the visible window ID when the user selects another native tab.
Rowla removed the previous ID and appended the new ID to the taskbar list.
The tile thus moved to the end of the list.

Rowla now transfers the previous taskbar position to the new window ID before it removes absent IDs.
The two windows must share an opaque native tab control ID, process ID, and application identity.
Ambiguous matches keep the normal discovery order.
No title or geometry comparison determines the relationship.

The same rule preserves manual order with either Space-order option.
A focused native tab also remains visible while the screen window list catches up with Accessibility focus.
The change adds no resident cache, native control requests, or thumbnail captures.
The bitmap cache limit remains 16 MiB and 32 images.

## Results

The Rust checks passed 57 tests, with three opt-in benchmarks ignored.
Rust format and Clippy checks passed without warnings.
The Python checks passed 23 tests, with one opt-in certificate check skipped.
Bundle compatibility checks passed.

The native UI check passed 24 tab changes with unchanged tile and neighbor frames.
It passed both Space-order options and immediate selected-tab feedback.
The full UI check also passed 624 hit tests and 606 feedback checks.

The native AppKit fixture check passed four tab changes across different physical window IDs.
The tile kept its position between two synthetic neighbors.
Utility activation, stale-target rejection, and exact inactive-tab close also passed.
The worker released all tracked Core Foundation objects.

An earlier run lost foreground focus during the utility activation assertion.
A separate repeat passed the complete native fixture check.
These checks used disposable AppKit windows. They did not directly control Ghostty.

## Installation

The installed app uses the same executable bytes as the tested bundle.
The installer accepted the previous code-signature identity.
The installed app restarted successfully at `/Users/sharif/Applications/Taskbar Rust.app`.

Executable SHA-256:

```text
43c18e9e849da6f61fbbc45021def9bb14cdbcd23a4852a79af9775e285281f2
```

Local raw reports are in the ignored `validation/native-tab-order/` directory.
The native fixture report is `after-sequential.json`.
The UI report is `native-ui.log`.
