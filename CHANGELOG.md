# Release notes

## 0.2.0-beta.2 — October 10, 2026

- Related utility windows and native tabs appear as compact bubbles on their main tile.
- Native tab changes preserve the tile position. Temporary gaps in window discovery preserve related controls.
- Chrome windows show local profile photos or initials.
- Hover **⌘Q** quits the application that owns the window.
- Window activation recovers from late Accessibility replies and discards obsolete targets.
- Shared snapshots, bounded native controls, and fewer temporary allocations reduce repeated work.
- The project guide and check scripts use one source for each procedure.

Apple Silicon, macOS 15.2+. Manual downloads use a development certificate with no Apple notarization.
The thumbnail cache remains limited to 16 MiB and 32 images. This is not a total process memory limit.

## 0.2.0-beta.1 — October 5, 2026

- The minimum supported macOS version changes to 15.2.
- Signed Shortcuts provide Sort Windows, Show Taskbars, and Hide Taskbars.
- Optional native features use runtime availability checks.
- CI covers Apple Silicon and Intel on macOS 15 and 26.

## 0.1.0-beta.1 — October 4, 2026

First public Rowla beta for Apple Silicon and macOS 26 or later.

- Native window buttons, compact titles, and direct window activation.
- A Sort button for preferred application order, with recent main windows before popups.
- Cached hover thumbnails, a compact bezel, and bounded capture and cache storage.
- Hover **⌘W** and thumbnail **×** window close, including minimized windows.
- Pinned icons only when an application has no discovered window.
- Start search, appearance preferences, window filters, and drag reorder.
- Start at login and automatic window resizing off for fresh preferences.
- Manual downloads, complete license notices, and a persistent development signature.

This beta is not notarized. First launch can require Open Anyway. Intel is not included. Independent installation and broader OS/window behavior still need beta feedback. Read the user guide for known limits.
