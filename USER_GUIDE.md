# Rowla

Pronounced **ROH-luh**. A native taskbar for macOS that puts each window within reach.

Rowla shows a button for each window. Use Sort to restore your preferred application order after a busy session. Preview a window before you switch to it, or close it directly from the bar.

Official downloads and support: <https://github.com/sharifhsn/rowla>

## Requirements

- macOS 15.2 or later for the current beta download.
- Apple Silicon: M1 or later for the beta ZIP. Intel source builds pass CI, but live controls need independent validation.
- Accessibility permission for window discovery and controls.
- Optional Screen Recording permission for thumbnails.

This is a free beta with a persistent development signature. Apple did not notarize this release. macOS can block first launch until you approve this specific app. Automatic updates are disabled. Use manual downloads for later beta versions.

The executable and app manifest both set a macOS 15.2 minimum. Newer optional APIs use runtime availability checks.
Read [compatibility](https://github.com/sharifhsn/rowla/blob/main/docs/COMPATIBILITY.md) for the API review and older-version limits.

## Installation

1. Download the ZIP archive from the official GitHub release.
2. Extract `Rowla.app`.
3. Move `Rowla.app` to your Applications folder.
4. Open Rowla.
5. If macOS blocks first launch, open System Settings → Privacy & Security.
6. Select Open Anyway for Rowla, then confirm Open.
7. Grant Accessibility access to Rowla in System Settings → Privacy & Security → Accessibility.
8. To enable thumbnails, grant Screen Recording access to Rowla in System Settings → Privacy & Security.
9. Quit and reopen Rowla if macOS requests a restart of the app after a permission change.

[Apple describes Open Anyway](https://support.apple.com/en-us/102445). Some managed Macs prohibit this exception. Rowla does not need you to disable Gatekeeper or SIP.

## Everyday controls

- Click a window button to activate or restore that window.
- Hover over a window button to see its thumbnail.
- Small bubbles at the top of a window button show related utility windows and native macOS tabs. Hover for their full names.
- Click a bubble to show its window or select its tab. Use the count bubble to open the full list.
- Press **⌘W** over a bubble to close its specific window or tab.
- Press **⌘W** with the pointer over a window button or thumbnail to close that window. This also works with minimized windows.
- Select the small **×** on a thumbnail to close its window.
- Use the **Sort** button immediately after Start to apply your preferred application order.
- Use Preferences → Sort to change the application order. Recent main windows come before popups in each application.
- Drag window buttons or pinned icons to change their order.
- Open **Start** to search for installed applications. Use the arrow keys and Return, or select an application.
- Select the Rowla menu-bar icon to open Preferences or quit Rowla.

Close controls close the selected window. They do not quit the application. An application can display its normal save-confirmation dialog.

Compact bubbles are on by default. Disable **Preferences → Taskbar → Show related windows as compact bubbles** to restore separate tiles.
Independent main windows retain separate tiles. Rowla groups a utility window only when its owner is clear.
Native tabs use the application's Accessibility tab controls. This includes AppKit tab groups used by applications such as Ghostty.
An inactive native tab exposes its name; its image becomes available after selection. Browser document tabs need application-specific support.

Chrome window icons show a small profile photo at the top right. A profile-color ring and tooltip help identify the profile.
The badge uses Chrome's local profile metadata and cached photo. A missing photo uses a colored initial.
Disable **Preferences → Taskbar → Show Chrome profile badges** to remove these badges.
The feature supports standard Chrome data folders and English accessibility titles. Unknown or ambiguous profiles receive no badge.

Pinned icons appear only when the application has no discovered window. A minimized or hidden window still counts as a window.

Cached thumbnails appear immediately. A first capture needs time. Rowla retains a last captured preview for a minimized window. A window that was already minimized before its first capture can have no thumbnail.

## Preview capture options

Open Preferences → Thumbnails to select **Preview capture: Stream** or **Snapshot**.
Stream is the default. It reuses live frames for a hovered window and stops after idle time.
Snapshot requests one image for each refresh. It can help you compare capture behavior on your Mac.

Both methods use the same 16 MiB / 32-image cache and cancellation rules.
Rowla permits only one native capture operation at a time. An unconfirmed native operation blocks another capture.
A minimized window retains its last image with either method.

## Shortcuts and Spotlight

1. Open Preferences → Menubar.
2. Select **Install Sort Shortcut…**, **Install Show Shortcut…**, or **Install Hide Shortcut…**.
3. Review the URL and Open URLs actions, then select **Add Shortcut** in Shortcuts.
4. Assign a keyboard shortcut through the Shortcuts details if you want one.

On first use, macOS can ask for permission to open Rowla. Select **Allow** for that shortcut.

Sort applies your order after Rowla receives its window list.
Show restores all bars. Hide hides all bars and enables the menu-bar icon for restoration.

The saved shortcuts can run from Spotlight. They start Rowla if necessary.
Each shortcut opens one local URL. It does not run a shell command or need Apple Intelligence.
You can also use an Open URLs action with `rowla://sort`, `rowla://show`, or `rowla://hide`.

## Preferences and privacy

Preferences controls appearance, bar scale, display layout, window visibility, pins, thumbnails, and window behavior.
Rowla automatically follows active Spaces. Its all-Spaces indicator marks window assignments, rather than a user-selectable Space filter.
Start at login and automatic window resizing are off for a new configuration. Enable them in Preferences if you want them.

The [full capability list](https://github.com/sharifhsn/rowla/blob/main/docs/CAPABILITIES.md) gives access paths, defaults, permissions, and limitations for each feature.

Rowla needs no account. It has no telemetry, advertising, or remote crash reports. Window previews stay in memory. Rowla does not save or transmit the preview images. The thumbnail cache has a 16 MiB and 32-image limit. Native capture buffers use additional memory.

Preferences and local diagnostics use `~/Library/Application Support/Taskbar Rust/`. The legacy name preserves compatibility with earlier Taskbar Rust builds. The internal bundle identifier remains `io.sharif.taskbarrust` for the same reason.

Rowla can import preferences from the original Taskbar on first use. An imported start-at-login preference can override the new default. Quit an earlier Taskbar Rust build before you open Rowla. They share preferences and an instance lock.

If you enable Dock or window-resize options, Rowla changes their behavior as described in Preferences. Quit Rowla to restore Dock settings that it changed. After a crash, the next launch restores its saved Dock state.

## Known beta limits

- Independent installation on another Mac still needs validation.
- External displays, sleep/wake, fullscreen, revoked permissions, and some application-specific window controls need broader beta feedback.
- Rowla uses optional private macOS functions for window identity and Spaces. OS changes can affect these features. Public API fallbacks can show an incomplete window list.
- Short local memory profiles are finite measurements. They do not establish that future memory spikes or leaks are impossible.

## Support and removal

Report bugs through <https://github.com/sharifhsn/rowla/issues>. Include the Rowla version, macOS version, affected application, and steps to reproduce the problem. Remove private window titles or other personal information before you attach screenshots or diagnostics.

To remove Rowla, disable Start at login in Preferences. Quit Rowla, then remove `Rowla.app`. If you also want to remove its preferences, remove `~/Library/Application Support/Taskbar Rust/` after you quit all earlier Taskbar Rust builds.

Rowla is an independent Rust/AppKit implementation inspired by [Taskbar](https://lawand.io/taskbar/). It is not affiliated with lawand.io or Apple. The app includes MIT and third-party license notices in `Contents/Resources/`.
