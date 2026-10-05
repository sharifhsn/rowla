# Rowla capabilities

This list describes the current public beta and source. It describes implemented behavior, rather than complete compatibility with every application.
The beta download needs macOS 26+ and Apple Silicon. Read [compatibility](COMPATIBILITY.md) for the reason and possible older targets.

## Window controls

Accessibility access supplies window discovery and controls. An application must expose the relevant window and action to macOS.

| Capability | Access | Behavior and limits |
| --- | --- | --- |
| One button per window | Taskbar | Shows an app icon and a single-line title with truncation. A tooltip supplies the full title |
| Activate and restore | Click a window button | Activates that specific window and restores it if minimized. Active feedback appears on the button |
| Close a window | Hover **⌘W**, thumbnail **×**, or context menu | Closes the selected window, including minimized windows. Save dialogs remain under the application's control |
| Middle-click close | Preferences → Taskbar | Enabled by default. The option controls middle-click on a window button |
| Minimize and fullscreen | Right-click a window button | Provides **Minimize / Restore** and **Toggle Full Screen**. Support depends on the application |
| Application controls | Right-click a window button | Provides **Hide Application** and the separate **Quit Application** action |
| Manual order | Drag window buttons | Changes their order. Preferences → Taskbar can disable drag reorder |
| Overflow navigation | Taskbar arrows or horizontal scroll | Keeps the strip on one line when the window count exceeds its width |

Close controls close a window. The explicit Quit Application command quits the app.
Browser content tabs remain under the browser's control. Rowla does not provide a browser-tab catalog.

## Order, apps, and launch controls

| Capability | Access | Behavior and limits |
| --- | --- | --- |
| Configured application order | Sort, immediately after Start | Applies saved app precedence when you click. Within each app, recent main windows precede dialogs and utility windows |
| Edit Sort order | Preferences → Sort, or right-click Sort | Adds, removes, and moves apps. Can copy the current taskbar order or Dock order |
| Unlisted apps | Sort | Follow configured apps in their current relative order |
| Continuous app grouping | Preferences → Taskbar | Keeps an app's buttons together. This option differs from the explicit Sort action |
| Pinned launch icons | Right-click → Keep in Taskbar | Appear when the app has no discovered window. Hidden, minimized, and other-Space windows can suppress a pin |
| Pin order and click behavior | Drag or right-click a pin | Supports launch/activate or a new-window command. New-window support depends on the application's menu |
| Browser private windows | Right-click a Chrome, Safari, or Firefox pin | Provides the browser-specific private-window command. Needs Accessibility and a compatible application menu |
| Application search | Start | Searches installed apps by name. Empty search shows Pinned, Recently Used, and All Applications |
| Keyboard launch | Start | Uses arrow keys and Return to select and open an application |
| Spotlight alternative | Preferences → Taskbar → start_action | Changes Start from Rowla's app menu to the Spotlight action |

## Displays, visibility, and appearance

| Capability | Access | Behavior and limits |
| --- | --- | --- |
| Multiple displays | Preferences → Taskbar | Places a bar on each eligible display. Can restrict bars to the main display or repeat all display windows on every bar |
| Current Space | Automatic | Filters windows to active Spaces. Optional private APIs supply Space identity. Public fallbacks can give an incomplete list |
| Space indicators | Preferences → Taskbar | Can mark windows assigned to multiple Spaces. This option does not select an arbitrary Space |
| Standard-tab visibility | Preferences → Taskbar | Can include windows that Rowla otherwise classifies as inactive standard tabs. The classification is a visibility heuristic |
| Hidden windows | Preferences → Taskbar | Can include hidden apps. A separate Advanced option excludes hidden Discord windows |
| App exclusions | Preferences → Blacklist, or window context menu | Removes selected applications from the window strip |
| Temporary or persistent bar hide | Scroll gesture or bar context menu | Can hide a bar for the current session or selected display. **Show All Taskbars** restores it |
| Fullscreen visibility | Automatic | Hides a bar when its display has a focused fullscreen window |
| Menu-bar controls | Rowla menu-bar icon / Preferences → Menubar | The icon opens Preferences. The Menubar page provides **Show All Taskbars** and can hide the icon |
| Light, dark, or system theme | Preferences → Appearance | Controls the theme, transparency, alignment, bar scale, font, and maximum button width |
| Optional title and state markers | Preferences → Appearance | Can hide titles, use minimized/hidden markers, and display thumbnail titles |
| Notification indicator | Preferences → Taskbar | Shows a dot when the Dock exposes an application badge. It does not provide a notification inbox |

## Previews, system options, and privacy

| Capability | Access | Behavior and limits |
| --- | --- | --- |
| Hover preview | Hover a window button | Needs Screen Recording access. Cached images appear immediately. A first or fresh capture needs time |
| Minimized preview | Hover a minimized window | Reuses the last captured image while it remains in the bounded cache. A window with no earlier capture can have no image |
| Preview appearance | Preferences → Thumbnails / Appearance | Controls preview size, title, font, and hover delay. The frame has a compact border and a soft hover highlight |
| Bounded preview resources | Automatic | Limits thumbnail bitmaps to 16 MiB and 32 entries. One capture stream uses a three-buffer queue. Total process memory is larger |
| Start at login | Preferences → Taskbar | Off for fresh preferences. macOS controls login registration and can require approval |
| Dock replacement option | Preferences → Advanced | Off for fresh preferences. Fully hides the Dock while Rowla runs. Quit restores its saved Dock values. After a crash, the next launch restores them |
| Avoid bar overlap | Preferences → Advanced | Optional automatic resize of overlapping windows. Off for fresh preferences. Needs Accessibility |
| Local diagnostics | About or context menu → Save Diagnostics | Records aggregate counts, timings, capability flags, and errors. Remove private data before a public attachment |
| Local crash reports | Preferences → Policies | Controls local reports. Rowla has no remote crash-report service |
| Manual updates | GitHub Releases | The public beta has no live update feed. Sparkle support in the source does not enable automatic updates for this download |
| Local use | Default | Needs no account. Has no telemetry or advertising. Previews stay in memory and never become an on-disk screenshot archive |

Imported preferences can differ from fresh defaults. [The user guide](../USER_GUIDE.md) explains installation, permissions, and removal.

## Source and contributor support

The MIT source includes pinned dependencies, build scripts, fixtures, microbenchmarks, and tests.
CI checks the portable Rust library on Linux and builds the app on Apple Silicon and Intel macOS runners.
Intel CI does not establish Intel daily-use compatibility. Older macOS versions still need runtime checks.

[CONTRIBUTING.md](../CONTRIBUTING.md) describes the human workflow.
[AGENTS.md](../AGENTS.md) gives coding agents the same commands, native invariants, and evidence requirements.

## Beta coverage

Permissions, application-specific close/fullscreen actions, external displays, Spaces, and sleep/wake still need broader independent feedback.
Short local memory measurements do not prove that every future native path is free of leaks or spikes.
Use the [support guide](SUPPORT.md) and [report forms](https://github.com/sharifhsn/rowla/issues/new/choose) for a specific failure.
