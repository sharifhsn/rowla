//! Shared profile badges. File reads and PNG decoding stay off the UI thread.
use super::*;
use crate::chrome_profiles::{AVATAR_SIDE, Browser, Catalog, Profile};
use block2::RcBlock;
use objc2_core_foundation::CFData;
use objc2_core_graphics::{
    CGBitmapInfo, CGColorRenderingIntent, CGColorSpace, CGDataProvider, CGImage, CGImageAlphaInfo,
    CGImageByteOrderInfo,
};
use std::collections::BTreeMap;

struct Cached {
    source: Profile,
    image: Retained<NSImage>,
}
type FileWorker = (mpsc::SyncSender<Vec<Browser>>, mpsc::Receiver<Vec<Catalog>>);
#[derive(Default)]
pub(super) struct Service {
    catalogs: Vec<Catalog>,
    images: BTreeMap<(Browser, String), Cached>,
    worker: Option<FileWorker>,
    requested: Vec<Browser>,
    next: Option<Instant>,
    pending: bool,
}
impl Service {
    pub(super) fn poll(&mut self, windows: &[Window], enabled: bool) -> bool {
        let mut active = Vec::with_capacity(4);
        if enabled {
            for browser in windows
                .iter()
                .filter_map(|w| Browser::for_bundle(&w.bundle))
            {
                if !active.contains(&browser) {
                    active.push(browser);
                }
            }
            active.sort();
        }
        let mut changed = false;
        if active.is_empty() && !self.catalogs.is_empty() {
            self.install(vec![]);
            changed = true;
        }
        if let Some((_, rx)) = &self.worker
            && let Ok(catalogs) = rx.try_recv()
        {
            self.pending = false;
            if active == self.requested {
                if self.catalogs != catalogs {
                    self.install(catalogs);
                    changed = true;
                }
            } else {
                self.next = None;
            }
        }
        if (active != self.requested
            || (!active.is_empty() && self.next.is_none_or(|next| Instant::now() >= next)))
            && !self.pending
        {
            if self.worker.is_none() && !active.is_empty() {
                self.worker = Some(crate::chrome_profiles::native::start());
            }
            if let Some((tx, _)) = &self.worker
                && tx.try_send(active.clone()).is_ok()
            {
                self.requested = active;
                self.pending = true;
                self.next = Some(Instant::now() + Duration::from_secs(30));
            }
        }
        changed
    }
    pub(super) fn install(&mut self, catalogs: Vec<Catalog>) {
        self.images.retain(|(browser, folder), cached| {
            catalogs.iter().any(|c| {
                c.browser == *browser
                    && c.profiles
                        .iter()
                        .any(|p| p.folder == *folder && p == &cached.source)
            })
        });
        self.catalogs = catalogs;
    }
    pub(super) fn diagnostics(&self) -> serde_json::Value {
        let profiles = self.catalogs.iter().flat_map(|c| &c.profiles);
        serde_json::json!({
            "profiles": profiles.clone().count(),
            "photos": profiles.clone().filter(|p| p.pixels.is_some()).count(),
            "pixel_bytes": profiles.filter_map(|p| p.pixels.as_ref()).map(|p| p.len()).sum::<usize>(),
            "cached_badges": self.images.len(),
            "pending": self.pending,
        })
    }
    pub(super) fn badge(
        &mut self,
        bundle: &str,
        title: &str,
    ) -> Option<(Retained<NSImage>, String)> {
        let browser = Browser::for_bundle(bundle)?;
        let profile = self
            .catalogs
            .iter()
            .find(|c| c.browser == browser)?
            .profile_for_title(title)?;
        let cached = self
            .images
            .entry((browser, profile.folder.clone()))
            .or_insert_with(|| Cached {
                source: profile.clone(),
                image: make_badge(profile),
            });
        Some((cached.image.clone(), profile.name.clone()))
    }
}
fn photo(profile: &Profile) -> Option<Retained<NSImage>> {
    let bytes = profile.pixels.as_ref()?;
    if bytes.len() != AVATAR_SIDE * AVATAR_SIDE * 4 {
        return None;
    }
    // SAFETY: the fixed-size BGRA buffer matches stride and height. CFData
    // copies it, and the provider/image retain their data after this call.
    let data = unsafe { CFData::new(None, bytes.as_ptr(), bytes.len() as isize) }?;
    let provider = CGDataProvider::with_cf_data(Some(&data))?;
    let color = CGColorSpace::new_device_rgb()?;
    let image = unsafe {
        CGImage::new(
            AVATAR_SIDE,
            AVATAR_SIDE,
            8,
            32,
            AVATAR_SIDE * 4,
            Some(&color),
            CGBitmapInfo(
                CGImageAlphaInfo::PremultipliedFirst.0 | CGImageByteOrderInfo::Order32Little.0,
            ),
            Some(&provider),
            std::ptr::null(),
            true,
            CGColorRenderingIntent::RenderingIntentDefault,
        )
    }?;
    Some(NSImage::initWithCGImage_size(
        NSImage::alloc(),
        &image,
        NSSize::new(12.0, 12.0),
    ))
}
fn make_badge(profile: &Profile) -> Retained<NSImage> {
    let picture = photo(profile);
    let color = NSColor::colorWithSRGBRed_green_blue_alpha(
        ((profile.color >> 16) & 255) as f64 / 255.0,
        ((profile.color >> 8) & 255) as f64 / 255.0,
        (profile.color & 255) as f64 / 255.0,
        1.0,
    );
    let local = profile
        .name
        .rsplit_once(" (")
        .map_or(profile.name.as_str(), |(_, local)| local);
    let initial = NSString::from_str(
        &local
            .chars()
            .next()
            .unwrap_or('•')
            .to_uppercase()
            .take(1)
            .collect::<String>(),
    );
    // SAFETY: AppKit supplies immutable global NSString keys.
    let keys = unsafe { [NSFontAttributeName, NSForegroundColorAttributeName] };
    let attrs = objc2_foundation::NSDictionary::from_slices(
        &keys,
        &[
            NSFont::boldSystemFontOfSize(7.0).as_ref(),
            NSColor::whiteColor().as_ref(),
        ],
    );
    let draw = RcBlock::new(move |_: NSRect| {
        NSColor::windowBackgroundColor().set();
        NSBezierPath::bezierPathWithOvalInRect(rect(0.0, 0.0, 12.0, 12.0)).fill();
        color.set();
        NSBezierPath::bezierPathWithOvalInRect(rect(0.75, 0.75, 10.5, 10.5)).fill();
        NSGraphicsContext::saveGraphicsState_class();
        NSBezierPath::bezierPathWithOvalInRect(rect(1.5, 1.5, 9.0, 9.0)).addClip();
        if let Some(image) = &picture {
            image.drawInRect_fromRect_operation_fraction(
                rect(1.5, 1.5, 9.0, 9.0),
                NSRect::ZERO,
                NSCompositingOperation::SourceOver,
                1.0,
            );
        } else {
            // SAFETY: the attribute keys have their documented NSFont/NSColor
            // values. This drawing block runs in AppKit's image context.
            unsafe {
                initial.drawInRect_withAttributes(rect(3.25, 1.5, 6.5, 9.0), Some(&attrs));
            }
        }
        NSGraphicsContext::restoreGraphicsState_class();
        objc2::runtime::Bool::YES
    });
    NSImage::imageWithSize_flipped_drawingHandler(NSSize::new(12.0, 12.0), false, &draw)
}
pub(super) fn frame(button: &ActionButton) -> Option<NSRect> {
    let bounds = button.bounds();
    let width = bounds.size.width - button.ivars().reserved_width.get();
    let image = button
        .cell()?
        .imageRectForBounds(rect(0.0, 0.0, width, bounds.size.height));
    let size = (image.size.width.min(image.size.height) * 0.5).min(12.0);
    if size < 5.0 || width < size + 2.0 {
        return None;
    }
    let x = (image.origin.x + image.size.width - size + 3.0)
        .min(width - size - 1.0)
        .max(1.0);
    let y = if button.isFlipped() {
        (image.origin.y - 3.0).max(1.0)
    } else {
        (image.origin.y + image.size.height - size + 3.0).min(bounds.size.height - size - 1.0)
    };
    Some(rect(x, y, size, size))
}
pub(super) fn draw(button: &ActionButton) {
    if let Some(image) = button.ivars().profile_badge.borrow().as_ref()
        && let Some(frame) = frame(button)
    {
        // Respect the NSButton coordinate system so the badge stays at the
        // visual top right in both flipped and unflipped native views.
        unsafe {
            image.drawInRect_fromRect_operation_fraction_respectFlipped_hints(
                frame,
                NSRect::ZERO,
                NSCompositingOperation::SourceOver,
                1.0,
                true,
                None,
            )
        };
    }
}
pub(super) fn apply(state: &Shared, button: &ActionButton, window: &Window) {
    let badge = {
        let mut s = state.borrow_mut();
        if s.config.chrome_profile_badges {
            s.profile_badges.badge(&window.bundle, &window.title)
        } else {
            None
        }
    };
    let mut current = button.ivars().profile_badge.borrow_mut();
    let image = badge.as_ref().map(|(image, _)| image);
    if current.as_ref().map(|image| std::ptr::from_ref(&**image))
        != image.map(|image| std::ptr::from_ref(&**image))
    {
        *current = image.cloned();
        NSView::setNeedsDisplay(button, true);
    }
    if let Some((_, name)) = badge {
        set_tooltip(
            button,
            &[
                &window.app,
                " — ",
                &window.title,
                "\nChrome profile: ",
                &name,
                "\n⌘W while hovering closes this window",
            ],
        );
    } else {
        set_tooltip(
            button,
            &[
                &window.app,
                " — ",
                &window.title,
                "\n⌘W while hovering closes this window",
            ],
        );
    }
}
