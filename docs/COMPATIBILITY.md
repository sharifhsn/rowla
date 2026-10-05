# macOS and architecture compatibility

## Current distribution

| Target | Current status |
| --- | --- |
| macOS 26+ on Apple Silicon | Public beta ZIP. Current local development and native checks use macOS 26 |
| macOS 26+ on Intel | Source builds and CI pass. No Intel beta ZIP or independent daily-use validation |
| macOS 14 and 15 | Plausible next targets from the API review. No supported download or runtime validation yet |
| macOS 13 | Needs capture changes and a full compatibility review. No supported download |
| macOS 12 or earlier | Needs more capture/login compatibility work and a framework audit. No supported download |
| Linux | Portable Rust configuration/model tests. No taskbar GUI |
| Windows | No taskbar GUI |

## Why the beta says macOS 26

The repository explicitly sets macOS 26 in two places:

- [.cargo/config.toml](../.cargo/config.toml) sets `MACOSX_DEPLOYMENT_TARGET = "26.0"` for the executable.
- [scripts/Info.plist](../scripts/Info.plist) sets `LSMinimumSystemVersion` to `26.0` for the app bundle.

The local build also reports `minos 26.0` in its Mach-O build command.
Thus, the current download has a real launch restriction. It is not only a documentation label.

The implementation review found no macOS 26-only requirement in the main taskbar, Sort, or preview features.
The current floor reflects the build configuration and validated environment. It does not establish the oldest technically possible version.
The reason for the original choice is not recorded in the source.

## API requirements found in the review

Review date: October 4, 2026. Apple documentation metadata and the local SDK headers agree on these versions.

| Current use | First macOS version | Effect |
| --- | --- | --- |
| ScreenCaptureKit single-window filter | 12.3 | Supplies the preview stream's window selection |
| `SCStreamConfiguration.capturesAudio` | 13.0 | The capture setup calls this property to disable audio |
| `SCStreamConfiguration.ignoreShadowsSingleWindow` | 14.0 | The capture setup calls this property without a runtime availability check |
| `SMAppService` | 13.0 | Supplies optional start-at-login registration. The code checks class availability |

Sources: [single-window filter](https://developer.apple.com/documentation/screencapturekit/sccontentfilter/init(desktopindependentwindow:)), [audio property](https://developer.apple.com/documentation/screencapturekit/scstreamconfiguration/capturesaudio), [shadow property](https://developer.apple.com/documentation/screencapturekit/scstreamconfiguration/ignoreshadowssinglewindow), and [login service](https://developer.apple.com/documentation/servicemanagement/smappservice).
The relevant implementation is in [capture.rs](../src/capture.rs) and [platform.rs](../src/platform.rs).

The unguarded shadow property makes macOS 14 a reasonable first compatibility target for the current capture code.
That is an engineering inference, rather than a complete API audit or a claim of supported macOS 14 behavior.
Optional private window/Spaces functions and framework dependencies also need checks on each target OS.
The embedded Sparkle framework declares macOS 12.0 as its minimum. It does not explain the macOS 26 floor.

## What older support needs

1. Audit every used native symbol and selector for the target OS.
2. Set the executable and app-bundle floors to the same intended version.
3. Build each architecture and inspect the binary and embedded framework requirements.
4. Exercise first launch, both permission paths, previews, and disposable window controls on the actual older OS.
5. Exercise minimized windows, multiple displays, Spaces, sleep/wake, and denied permissions.
6. Publish a separate tested release with exact OS and architecture coverage.

A lower deployment target alone does not complete these checks. CI on macOS 26 does not substitute for an older-OS runtime.
The current beta remains a macOS 26+ release until a separate compatibility release completes this work.
