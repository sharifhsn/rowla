# Chrome profile badges

Checks on October 8, 2026 used Apple Silicon and macOS 26.6.2 (25G83).
The bundle supports macOS 15.2 and later.

Chrome windows show a 12-point profile badge at the top right of the app icon.
The badge uses a local cached photo and profile color, or a colored initial.
The tooltip shows the full profile name.
**Preferences → Taskbar → Show Chrome profile badges** controls the feature.

Separate catalogs cover Stable, Beta, Dev, and Canary.
Custom data folders and localized window-title formats are unsupported.
Unknown, ambiguous, Incognito, Guest, and Isolated titles receive no profile badge.
The live check covered Stable only.

Profile assignment uses Chrome's [accessible title](https://github.com/chromium/chromium/blob/main/chrome/browser/ui/views/frame/browser_view.cc),
[English suffixes](https://github.com/chromium/chromium/blob/main/chrome/app/generated_resources.grd),
and [profile display-name rules](https://github.com/chromium/chromium/blob/main/chrome/browser/profiles/profile_attributes_entry.cc).
Page text never determines the profile.

One file worker reads local metadata and PNGs every 30 seconds while Chrome windows remain present.
The feature makes no network requests and adds no permission or window capture.
Unchanged photos share pixels. Removed profiles release cached badges.
Disable the feature to clear its catalogs and badges.

Resource limits are 32 profiles per channel, 2 MiB per metadata file, and 256 KiB per PNG.
Source images cannot exceed 1024 × 1024 pixels.

Each decoded photo contains 4096 bytes. Four catalogs retain at most 512 KiB of current pixels.
One request and one reply use bounded queues. Each matched profile shares one native badge.
Native decode and handoff use additional memory. The thumbnail cache remains limited to 16 MiB and 32 images.

## Repeat the UI check

```sh
scripts/build.sh
TASKBAR_QA_RENDER_DIR="$PWD/validation/chrome-profile-badges" \
  python3 scripts/check-native-ui.py dist/Rowla.app
```

Synthetic checks passed profile assignment, minimized badges, photo fallback, shared images across 100 renders, bounds, hit targets, and toggle behavior.
The complete report required `passed:true`.
Rust passed 53 tests, with three benchmarks ignored.
Format, Clippy, compatibility, 23 Python tests, 624 UI hit tests, and 606 feedback checks passed.
One optional certificate test was skipped.

The installed app loaded five photos into 20,480 bytes of shared pixels.
Eighteen RSS samples covered 36 seconds.
RSS started at 114.1 MiB, peaked at 115.0 MiB, and ended at 109.1 MiB.
This finite check does not establish absence of future leaks or spikes.
