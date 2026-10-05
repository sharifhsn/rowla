# macOS and architecture compatibility

## Current distribution

The 0.2 release sets **macOS 15.2+** as its minimum. The public beta ZIP supports Apple Silicon.
The executable and app manifest both declare 15.2.
The original 0.1 beta retains its macOS 26 requirement and unchanged release files.

| Target | Coverage |
| --- | --- |
| macOS 15.2+ on Apple Silicon | Supported floor for the 0.2 beta. CI includes a macOS 15 runner |
| macOS 15.2+ on Intel | Source builds. CI includes macOS 15 Intel. No Intel public beta ZIP |
| macOS 26 | Local native fixture checks use 26.6.2. CI includes both architectures |
| macOS 27 | Optional capture-state selector uses a runtime check. No local 27 desktop check |
| macOS 14 or earlier | Unsupported. More API and maintenance work is necessary |
| Linux | Portable Rust configuration/model checks. No taskbar GUI |
| Windows | No taskbar GUI |

CI exercises native UI fixtures and required selector availability without capture or Accessibility permissions.
CI does not establish first-launch grants, application-specific controls, displays, Spaces, or daily-use reliability.
A macOS 15 runner does not prove behavior on the exact 15.2 point release.

## Binary requirements

- [.cargo/config.toml](../.cargo/config.toml) sets `MACOSX_DEPLOYMENT_TARGET = "15.2"`.
- [scripts/Info.plist](../scripts/Info.plist) sets `LSMinimumSystemVersion` to `15.2`.

[scripts/check-compatibility.py](../scripts/check-compatibility.py) examines every bundled Mach-O binary and architecture.
It rejects higher minimum versions, inconsistent executable/bundle floors, and missing architecture floors.
The embedded Sparkle framework and helpers declare macOS 12.0. They do not raise Rowla's floor.
The native `--check-compatibility` command checks required selectors on the current OS without permission prompts.

## Required and optional APIs

| API | First macOS version | Use |
| --- | --- | --- |
| ScreenCaptureKit single-window filter | 12.3 | Selects a window for capture |
| `SCStreamConfiguration.capturesAudio` | 13.0 | Disables audio |
| `SMAppService` | 13.0 | Optional start at login |
| Cooperative activation | 14.0 | Click activation and application launch |
| `ignoreShadowsSingleWindow` | 14.0 | Removes shadows from capture |
| `SCScreenshotManager.captureImage` | 14.0 | Optional Snapshot preview method |
| Capture active/inactive callbacks | 15.2 | Handles unavailable capture sources |
| Compact native control metrics | 26.0 | Optional Preferences layout, runtime checked |
| `SCStream.isCapturing` | 27.0 | Optional diagnostics, runtime checked |

The [feature assessment](MACOS_FEATURE_RESEARCH.md) gives Apple sources and the selection rationale.
An inactive event is not a minimized-window event or native stop acknowledgment.
A false capture-state value does not prove buffer release. Confirmed stop completion remains necessary.

## Why the first beta said macOS 26

The original executable and app manifest both declared 26.0. That was a real launch restriction.
The source review found no macOS 26-only requirement in the main taskbar, Sort, or previews.
The reason for the original choice is not recorded in the source.
The 0.2 release changes the binary floor and adds compatibility gates, with matching documentation.

## What older support needs

1. Audit each native symbol, selector, and dependency for the target OS.
2. Set consistent executable and bundle minimum versions.
3. Examine every bundled binary on each architecture.
4. Exercise permissions, previews, and disposable window controls on the target OS.
5. Exercise minimized windows, displays, Spaces, sleep/wake, and denied permissions.
6. Record exact OS versions and any behavior that remains unverified.

Private window and Spaces functions can change between releases. Public fallbacks can produce an incomplete window list.
macOS 14 remains a possible technical target. Rowla supports 15.2 to retain capture lifecycle feedback and limit the maintenance matrix.
