# macOS features and the next compatibility target

Assessment date: October 5, 2026. Source review: public Rowla commit `56e917f`.

## Recommendation

**Use macOS 15.2+ as the target for the next supported public release.**
Keep macOS 26 and 27 features optional, with runtime availability checks.
This report records the original research. The 0.2 implementation adopts this minimum.
Read [compatibility](COMPATIBILITY.md) for current build and runtime coverage.

Apple lists **macOS 27 Golden Gate 27.0.1** as the latest public version.
Apple released 27 on September 14, 2026, and 27.0.1 on September 28.
The assessment proceeds backward through 27, 26, 15, 14, 13, and 12.3.
Sources: [current macOS versions](https://support.apple.com/en-us/109033) and [release history](https://support.apple.com/en-us/100100).

macOS 14 is a plausible technical minimum for the current code.
It supplies the unguarded shadow property and two useful APIs for future click and capture work.
macOS 15.2 adds capture lifecycle callbacks, which give the capture worker more direct information about unavailable content.
For a maintained public app, 15.2 is the better balance between compatibility and future changes.

The update history also favors Sequoia for the public support target.
Apple lists updates for macOS 27, 26, and 15 on September 28, 2026.
Its most recent listed Sonoma update is August 6, 2026.

These dates do not prove a Sonoma end-of-support date or a guaranteed future support period.
This recommendation uses the observed update history, rather than an assumed three-version support policy.
Source: [Apple security releases](https://support.apple.com/en-us/100100).

## Assessment from newest to oldest

The table separates proposed additions from current framework use.
API introduction versions come from Apple documentation metadata, with local SDK headers as a second check for selected APIs.

| Release | Useful additions for Rowla | Decision |
| --- | --- | --- |
| **27** | Native capture-state inspection. Sidecar touch gestures and explicit touch/scroll relationships | Optional diagnostics first. Touch support needs its own interaction checks. No reason to require 27 for desktop use |
| **26** | Compact control metrics. Spotlight actions through App Intents. Native Liquid Glass and more screenshot options | Compact metrics and explicit Sort actions are useful. Keep visual effects optional and measure their cost |
| **15 / 15.2** | System window tiling. Capture active/inactive callbacks arrive in **15.2** | Recommend **15.2** as the public support target. Exercise Rowla alongside system tiling |
| **14** | Cooperative app activation. Single-image capture. System content picker. Existing single-window shadow control | Strong performance experiment target and plausible technical minimum. Older public support needs a separate maintenance decision |
| **13** | `SMAppService` for login. Capture audio configuration | Login already exists. Support needs guards or alternatives for the current 14-only capture call, plus older capture and activation paths |
| **12.3** | Initial ScreenCaptureKit support | Possible basis for a larger compatibility project. It loses the 13/14 additions and needs more framework, login, and capture work |

Sources: [AppKit updates](https://developer.apple.com/documentation/updates/appkit), [ScreenCaptureKit updates](https://developer.apple.com/documentation/updates/screencapturekit), [login service](https://developer.apple.com/documentation/servicemanagement/smappservice), and [single-window filter](https://developer.apple.com/documentation/screencapturekit/sccontentfilter/init(desktopindependentwindow:)).

## Highest-value experiments

### 1. Review the window activation path — macOS 14+

Rowla calls `activateWithOptions:2` in [platform.rs](../src/platform.rs).
The value `2` means `NSApplicationActivateIgnoringOtherApps`.
Apple deprecated this flag in macOS 14 and states that it has no effect.
The macOS 14 cooperative APIs are `yieldActivation(to:)` and `activate(from:options:)`.
Sources: [activation options](https://developer.apple.com/documentation/appkit/nsapplication/activationoptions/activateignoringotherapps), [yield activation](https://developer.apple.com/documentation/appkit/nsapplication/yieldactivation(to:)), and [request activation](https://developer.apple.com/documentation/appkit/nsrunningapplication/activate(from:options:)).

This is a relevant candidate for the reported intermittent click failures.
It is not a proven cause or a guaranteed repair.
Rowla uses a nonactivating panel and does not always own the active application state.
Apple does not guarantee immediate activation, even when a request succeeds.
The experiment needs fixture checks with another active app, minimized windows, Spaces, and active computer-use sessions.

### 2. Compare single-image capture with the stream path — macOS 14+

`SCScreenshotManager.captureImage(contentFilter:configuration:completionHandler:)` supplies one image for a selected window.
It is a candidate for cold previews and occasional cache refreshes.
The current [capture worker](../src/capture.rs) uses one stream, a three-buffer queue, and one latest frame.
A screenshot request does not automatically cost less or return faster than that stream.
Source: [single-image capture](https://developer.apple.com/documentation/screencapturekit/scscreenshotmanager/captureimage(contentfilter:configuration:completionhandler:)).

Compare cold-hover latency, rapid window changes, repeated hover latency, peak memory, and steady memory after hover ends.
Preserve the 16 MiB / 32-image cache, deadlines, failure backoff, and obsolete-reply cancellation.
Keep at most one native capture operation in flight.
This limit also applies when a timeout leaves completion uncertain.

Keep the last available minimized-window image inside the same cache limits.
An unavailable first image remains possible. The new API does not promise live minimized-window content.

### 3. Add capture lifecycle feedback — macOS 15.2+

`streamDidBecomeInactive(_:)` reports when all shared windows exit.
`streamDidBecomeActive(_:)` reports when a shared window reopens after all shared windows close.
These callbacks can help the capture worker suspend unnecessary work and reconcile its state.

Apple does not describe them as minimize/restore notifications.
They do not replace a confirmed stream-stop result.
Sources: [inactive callback](https://developer.apple.com/documentation/screencapturekit/scstreamdelegate/streamdidbecomeinactive(_:)) and [active callback](https://developer.apple.com/documentation/screencapturekit/scstreamdelegate/streamdidbecomeactive(_:)).

### 4. Preserve compact native controls — macOS 26+

`NSView.prefersCompactControlSizeMetrics` selects control metrics consistent with macOS 15 and earlier.
This fits Rowla's compact appearance, particularly in Preferences and future native controls.
Current taskbar buttons already use explicit frames. This API is not a proven repair for previous height or text-wrap bugs.
Source: [compact control metrics](https://developer.apple.com/documentation/appkit/nsview/preferscompactcontrolsizemetrics).

### 5. Expose explicit actions in Shortcuts and Spotlight

Useful initial actions are **Sort windows**, **Show taskbars**, and **Hide taskbars**.
They can call the current command paths, with no model or continuous background scan.
The `AppIntent` framework starts at macOS 13. Direct actions in Spotlight on Mac arrive with macOS 26.
Sources: [AppIntent](https://developer.apple.com/documentation/appintents/appintent) and [Shortcuts and Spotlight on Mac](https://developer.apple.com/videos/play/wwdc2025/260/).

The Rust app needs a small Swift integration and build checks before this becomes a supported feature.
The initial actions need no window-title index. This keeps the change small and avoids more stored desktop metadata.
User-directed actions fit the current explicit Sort behavior.

### 6. Compare logical and native capture state — macOS 27+

`SCStream.isCapturing` reports whether the stream currently captures screen content.
It can supply optional diagnostic evidence when Rowla's logical state and native state disagree.
A false value alone does not prove that macOS released every buffer or completed an uncertain stop operation.
Source: [native capture state](https://developer.apple.com/documentation/screencapturekit/scstream/iscapturing).

## Optional features and poor fits

| Feature | Assessment |
| --- | --- |
| macOS 27 Sidecar gesture APIs | Useful if touch becomes a supported input. Cursor and mouse-click correctness still need direct regression checks. [Apple touch guidance](https://developer.apple.com/documentation/technotes/tn3212-adopting-gesture-recognizers-for-sidecar-touch-support) |
| macOS 26 Liquid Glass | A small optional preview bezel can fit. Measure CPU/GPU and memory cost, and respect Reduce Transparency. [Native glass view](https://developer.apple.com/documentation/appkit/nsglasseffectview) |
| macOS 26 screenshot configuration | Useful for a future crop/shadow experiment. Prefer SDR images and no file output for ordinary previews. [Screenshot configuration](https://developer.apple.com/documentation/screencapturekit/scscreenshotconfiguration) |
| macOS 15 system window tiling | Rowla can complement it. The feature does not establish a public API to tile arbitrary foreign application windows. [AppKit changes](https://developer.apple.com/documentation/updates/appkit) |
| macOS 14 system content picker | Useful for an optional preview mode limited to selected content. It needs user selection and does not silently grant capture access to every window. [Content picker](https://developer.apple.com/documentation/screencapturekit/sccontentsharingpicker) |
| macOS 27 rolling clip capture | Adds a video-buffer product feature that ordinary thumbnails do not need. Do not add it to the preview path. [Clip output](https://developer.apple.com/documentation/screencapturekit/scclipbufferingoutput) |
| HDR, microphone capture, recording, and local language models | No requirement for the current taskbar workflow. Defer them until a user need and resource budget justify their cost |

## Implementation selection

The 0.2 source implements capture lifecycle feedback, cooperative activation, optional snapshot capture, compact metrics, and capture-state diagnostics.
Snapshot capture remains selectable, with Stream as the default. Newer metrics and capture-state APIs use runtime checks.
Signed URL-based shortcuts provide explicit Sort, Show, and Hide controls without a Swift runtime or resident helper.
The App Intents prototype was discoverable locally, but actual invocation failed with a helper communication error under the available signing setup.
It is omitted from this beta. Native App Intents and direct action discovery remain candidates for a future release with verified signing and invocation.

The preview bezel remains compact and inexpensive. Liquid Glass needs a visual benefit and measured resource budget before adoption.
Video, HDR, microphone capture, language models, and touch-specific features have no established need in this workflow.

## Compatibility validation

The [compatibility checklist](COMPATIBILITY.md#what-older-support-needs) remains necessary.
The executable and app bundle now declare macOS 15.2.
CI includes macOS 15 and 26 on Apple Silicon and Intel. Permission-dependent checks still need a desktop device.

Newer Objective-C classes and selectors need availability checks before use on an older OS.
New framework dependencies also need suitable link/load behavior.
The pinned Rust bindings do not expose every macOS 27 API. Each new call needs a binding and availability audit.
Private window/Spaces functions need checks on each supported OS, with public fallback behavior in the matrix.

The first compatibility release needs Apple Silicon and Intel build checks plus native results for its declared versions.
Start with macOS 15.2+ support, then consider macOS 14 only if user demand and maintenance capacity justify another target.
