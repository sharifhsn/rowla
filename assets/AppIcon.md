# Rowla app icon

`AppIcon.png` is the source artwork. `scripts/build-icon.sh` uses macOS `sips`
and `iconutil` to produce the standard 16–1024 pixel ICNS representations before
bundle signing. AppKit selects representations through `CFBundleIconFile`; the
taskbar does not retain a separate full-size bitmap. `src/ui/brand.rs` draws the
matching 18-point template menu-bar symbol once, without loading the large source
artwork into the live menu-bar control.

Generated with the built-in image-generation tool on October 3, 2026.

Initial prompt:

> Use case: logo-brand. Asset type: final macOS app icon for Taskbar Rust, a small native window taskbar utility. Create one polished square app icon, 1024x1024, full bleed. A deep slate blue rounded macOS app tile with subtle dimensional material, a large clean ivory window outline above a compact row of three window/task tiles along its bottom edge. One task tile has a restrained vivid cyan accent, the others ivory. The symbol should feel purpose-built for managing open windows: orderly, precise, approachable, legible at 16px. Center the mark generously, use bold simple geometry, minimal detail, subtle soft studio lighting, rounded corners, and restrained depth like a well-made native macOS utility icon. No text, letters, Rust gear, mascot, badge, sparkles, mockup, outer background, cast shadow outside the app tile, or multiple variants. Fill the entire square canvas with the blue app tile so it can be masked cleanly for macOS; no white margins.

Transparency edit prompt:

> Preserve the exact app icon design, all geometry, the ivory window and task tiles, the cyan accent and the blue material. Change only the outside of the large rounded blue app tile: remove the flat blue background visible in the four outer corners and make those corner regions genuinely transparent. The rounded tile itself must remain opaque and retain its smooth edge. Do not crop, add margins, add a shadow outside the tile, redesign or change colors.
