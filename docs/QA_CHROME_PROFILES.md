# Chrome profile badges

Original checks: October 8, 2026, Apple Silicon, macOS 26.6.2 (25G83). Minimum supported version: macOS 15.2.

Chrome icons show a profile photo and color ring. A missing photo uses a colored initial.
The tooltip shows the profile name. The badge keeps the window click and close targets.
Use **Preferences → Taskbar → Show Chrome profile badges** to control this feature.

Rowla uses Chrome's [accessible window title](https://github.com/chromium/chromium/blob/main/chrome/browser/ui/views/frame/browser_view.cc),
[English title suffixes](https://github.com/chromium/chromium/blob/main/chrome/app/generated_resources.grd), and
[profile display names](https://github.com/chromium/chromium/blob/main/chrome/browser/profiles/profile_attributes_entry.cc).
Ambiguous names and private windows receive no badge. Custom data folders and localized titles need separate support.

One file worker reads local metadata every 30 seconds while Chrome windows are present.
Unchanged photos share pixel buffers. Removed profiles release their cache entries.
The feature needs no network access, extension, new permission, or window capture.

Resource limits:

- Four channels, with 32 profiles each.
- 2 MiB per metadata file and 256 KiB per PNG, with source dimensions at most 1024 × 1024.
- 32 × 32 BGRA pixels per photo. Current catalogs contain at most 512 KiB of photo pixels.
- One request and one reply per queue, and one native badge per matched profile.

Native objects and catalog handoff need more memory. The thumbnail cache keeps its 16 MiB and 32-image limits.

Repeat the synthetic UI check:

```sh
scripts/build.sh
TASKBAR_QA_RENDER_DIR="$PWD/validation/chrome-profile-badges" \
  python3 scripts/check-native-ui.py dist/Rowla.app
```

Profile assignment, minimized windows, distinct photos, image reuse, fallback, badge bounds, click targets, and toggle controls passed.
The UI check passed 624 hit tests and 606 feedback checks. Rust passed 53 tests, with three ignored benchmarks.
Python passed 23 tests, with one certificate test skipped. Format, Clippy, and compatibility checks passed.

The live check covered Chrome Stable. Five photos used 20,480 bytes of shared pixels.
Eighteen RSS samples over 36 seconds started at 114.1 MiB, peaked at 115.0 MiB, and ended at 109.1 MiB.
This short check does not establish long-term memory behavior. Installation kept the signature identity and matched the tested executable.
