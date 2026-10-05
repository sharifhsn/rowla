<p align="center"><img src="assets/icon.png" width="128" alt="Rowla app icon"></p>

# Rowla

**Every window within reach.** Pronounced **ROH-luh**.

A small native macOS taskbar. See your open windows, preview them, and restore your preferred application order with one click.

[**Download for Apple Silicon**](https://github.com/sharifhsn/rowla/releases/download/v0.1.0-beta.1/Rowla-0.1.0-beta.1-macos-arm64.zip) · [Release notes](https://github.com/sharifhsn/rowla/releases/tag/v0.1.0-beta.1) · [User guide](USER_GUIDE.md) · [Report a bug](https://github.com/sharifhsn/rowla/issues/new/choose)

**Requires macOS 26+ and Apple Silicon. Free beta.** This release uses a persistent development signature. Apple did not notarize it. macOS can require **Open Anyway** on first launch. Automatic updates are disabled.

## What it does

- Gives each window its own button, with a compact, single-line title.
- Restores your preferred application order with **Sort**, directly beside Start. Recent main windows come before popups.
- Shows cached thumbnails immediately, with a compact bezel and a soft hover highlight. Fresh captures need time.
- Closes the hovered window with **⌘W**, including minimized windows. The small **×** closes the window too.
- Hides a pinned application icon when that application already has a discovered window.
- Provides application search, drag reorder, appearance preferences, and display and Space filters.

Rowla uses native AppKit through Rust. It needs no account, has no telemetry, and keeps window previews in memory. The thumbnail cache has a **16 MiB / 32-image limit**. Native capture buffers use additional memory.

## Get started

1. Download the ZIP archive and extract `Rowla.app`.
2. Move `Rowla.app` to Applications and open it.
3. If macOS blocks it, open **System Settings → Privacy & Security → Open Anyway** for Rowla.
4. Grant **Accessibility** access for window controls.
5. To use thumbnails, grant **Screen Recording** access. Quit and reopen the app if macOS requests it.

[Apple documents Open Anyway](https://support.apple.com/en-us/102445). Some managed Macs prohibit this exception. Keep Gatekeeper and SIP enabled.

Close actions close the selected window, rather than the whole application. Its normal save-confirmation dialog can appear.

Use the Rowla menu-bar icon for Preferences or Quit. Start at login and automatic window resizing are off for fresh preferences. Earlier Taskbar Rust builds share the same preferences and app identity. Quit the earlier build before you open Rowla.

## Beta status

The initial release includes local Rust tests, strict Clippy checks, native UI checks, package-signature checks, and an archive round-trip check. It still needs feedback from other Macs. Intel is not included in this release.

Cached previews of minimized windows use the last captured image. A window already minimized before its first capture can have no thumbnail. OS updates can affect optional private window and Spaces functions. External displays, sleep/wake, fullscreen, and application-specific window controls need broader feedback. Finite local memory profiles do not establish zero leaks or zero future spikes.

Updates use manual versioned downloads during the beta. Quit Rowla before you replace its app bundle. Download only from this repository's Releases page.

## Support, privacy, and license

This is the official repository for downloads and public bug reports. Development source is currently private.

Read the [user guide](USER_GUIDE.md) and [privacy statement](PRIVACY.md). Report problems through [Issues](https://github.com/sharifhsn/rowla/issues). Include the version, macOS version, affected application, and reproduction steps. Remove personal information before you attach diagnostics or screenshots.

The app uses the [MIT license](LICENSE) and includes [third-party notices](THIRD_PARTY_NOTICES.md). The app icon was generated for this project. Rowla is an independent implementation inspired by [Taskbar](https://lawand.io/taskbar/), with no affiliation to lawand.io or Apple.
