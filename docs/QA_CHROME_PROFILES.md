# Chrome profile badges

Local checks on October 8, 2026 used Apple Silicon and macOS 26.6.2 (25G83).
The tested bundle retains its macOS 15.2 minimum.

Each matched Chrome window shows a 12-point badge at the top right of its app icon.
The badge uses a local cached profile photo and the profile's highlight color.
A missing or unsupported photo uses a colored initial. The tooltip supplies the full profile name.
The badge keeps the same window hit target, hover-close target, and single-line layout.

The feature is on by default. **Preferences → Taskbar → Show Chrome profile badges** controls it.
Standard Chrome Stable, Beta, Dev, and Canary data folders have separate catalogs.

Custom `--user-data-dir` folders and localized accessibility title formats need future support.
Unknown or ambiguous profile names receive no badge. Incognito, Guest, and Isolated titles do not identify a normal profile.
The local live check covered Stable only.

Chrome supplies the profile name through its [accessible window title](https://github.com/chromium/chromium/blob/main/chrome/browser/ui/views/frame/browser_view.cc).
The [English title resources](https://github.com/chromium/chromium/blob/main/chrome/app/generated_resources.grd) define the profile and private-window suffixes.
Rowla matches the exact profile name after the canonical browser suffix.
The local metadata uses Chrome's [profile display-name rules](https://github.com/chromium/chromium/blob/main/chrome/browser/profiles/profile_attributes_entry.cc).
It does not infer a profile from page text.

One lazy file worker reads `Local State` and cached profile PNG files.
The typed parser discards unrelated JSON fields. The feature makes no network requests and needs no extension or new permission.
The worker checks metadata at 30-second intervals while Chrome windows remain in the window list.
Unchanged photo files reuse shared pixel buffers. Removed profiles and closed browser channels release their cache entries.

Disable the feature to clear its catalog and native badges.

Resource bounds:

- At most 32 profiles per channel and four channels.
- A 2 MiB metadata file limit.
- A 256 KiB PNG file limit and maximum source dimensions of 1024 × 1024.
- Exactly 32 × 32 BGRA pixels per decoded photo, or 4096 bytes.
- At most 512 KiB of current profile pixels across all four catalogs.
- One request and one reply in bounded queues.
- One shared native badge per matched profile, rather than one image per window.

Native framework objects, source decode buffers, and a changed catalog during handoff use additional memory.
The current thumbnail cache retains its independent 16 MiB / 32-image limit.
Profile badges create no additional window captures.

To repeat the synthetic native UI check:

```sh
scripts/build.sh
TASKBAR_QA_RENDER_DIR="$PWD/validation/chrome-profile-badges" \
  python3 scripts/check-native-ui.py dist/Rowla.app
```

The fixture uses synthetic profile photos and names. It saves no desktop or account photo.
Its checks passed exact profile assignment, minimized-window badges, distinct photos, shared images across 100 renders, and a missing-photo fallback.
Badge bounds, badge hit targets, hover-close targets, toggle-off behavior, and the small icon-only layout passed.
AppKit rounded the nominal 19.2-point scaled panel to 20 points on this display.
The checker now requires a complete JSON report with `passed:true`, in addition to a successful child exit code.

All native UI regression checks passed: 624 hit tests, zero hit misses, and 606 immediate feedback checks without failures.
Rust checks passed 53 tests, with three opt-in benchmarks ignored. `cargo fmt` and Clippy checks passed without warnings.
Python checks passed 23 tests, with one opt-in certificate check skipped. Binary compatibility checks passed.

The installed app loaded five local profile photos into 20,480 bytes of shared pixels.
A visible Chrome window received its native badge. Accessibility and Screen Recording grants remained valid.
Eighteen RSS samples across 36 seconds started at 114.1 MiB, peaked at 115.0 MiB, and ended at 109.1 MiB.
This finite check found no continued memory growth across a metadata refresh. It does not prove the absence of all future leaks or spikes.

Installation preserved the certificate identity. The installed executable matched the tested bundle:

```text
cbe2d895f38fab04007e94ce54fcbc665766a5cb8b4680666f5abc0f0c15ba45
```
