//! One on-demand file worker. Only small owned pixels cross to the UI.
use super::*;
use objc2_core_foundation::CFData;
use objc2_core_graphics::{
    CGBitmapContextCreate, CGColorSpace, CGContext, CGImage, CGImageAlphaInfo, CGImageByteOrderInfo,
};
use objc2_image_io::CGImageSource;
use std::{fs::File, io::Read, sync::mpsc, time::SystemTime};

const MAX_PICTURE_BYTES: u64 = 256 * 1024;
fn read(path: &Path, max: u64) -> Option<Vec<u8>> {
    let file = File::open(path).ok()?;
    if file.metadata().ok()?.len() > max {
        return None;
    }
    let mut bytes = Vec::new();
    file.take(max + 1).read_to_end(&mut bytes).ok()?;
    (bytes.len() as u64 <= max).then_some(bytes)
}
pub(crate) fn decode(bytes: &[u8]) -> Option<Arc<[u8]>> {
    // Chrome stores GAIA pictures as PNG. Reject large source dimensions before
    // native decoding, and keep only a fixed-size bitmap after that decode.
    if bytes.len() < 24
        || bytes.len() as u64 > MAX_PICTURE_BYTES
        || &bytes[..8] != b"\x89PNG\r\n\x1a\n"
        || &bytes[12..16] != b"IHDR"
    {
        return None;
    }
    let width = u32::from_be_bytes(bytes[16..20].try_into().ok()?);
    let height = u32::from_be_bytes(bytes[20..24].try_into().ok()?);
    if width == 0 || height == 0 || width > 1024 || height > 1024 {
        return None;
    }
    // SAFETY: CFData copies this live buffer. ImageIO receives no options.
    // The typed results own their native objects on this worker.
    let data = unsafe { CFData::new(None, bytes.as_ptr(), bytes.len() as isize) }?;
    let source = unsafe { CGImageSource::with_data(&data, None) }?;
    let image = unsafe { source.image_at_index(0, None) }?;
    if CGImage::width(Some(&image)) != width as usize
        || CGImage::height(Some(&image)) != height as usize
    {
        return None;
    }
    let color = CGColorSpace::new_device_rgb()?;
    let mut pixels = vec![0u8; AVATAR_SIDE * AVATAR_SIDE * 4];
    // SAFETY: pixels has exactly height × stride bytes. The context drops
    // before this buffer moves, and synchronous draw_image finishes all writes.
    let context = unsafe {
        CGBitmapContextCreate(
            pixels.as_mut_ptr().cast(),
            AVATAR_SIDE,
            AVATAR_SIDE,
            8,
            AVATAR_SIDE * 4,
            Some(&color),
            CGImageAlphaInfo::PremultipliedFirst.0 | CGImageByteOrderInfo::Order32Little.0,
        )
    }?;
    CGContext::draw_image(
        Some(&context),
        objc2_core_foundation::CGRect::new(
            objc2_core_foundation::CGPoint::ZERO,
            objc2_core_foundation::CGSize::new(AVATAR_SIDE as f64, AVATAR_SIDE as f64),
        ),
        Some(&image),
    );
    drop(context);
    Some(pixels.into())
}
#[derive(PartialEq)]
struct Stamp {
    path: PathBuf,
    len: u64,
    modified: SystemTime,
}
struct Picture {
    stamp: Stamp,
    pixels: Option<Arc<[u8]>>,
}
#[derive(Default)]
struct CachedCatalog {
    catalog: Option<Catalog>,
    pictures: BTreeMap<String, Picture>,
}
impl CachedCatalog {
    fn refresh(&mut self, browser: Browser, root: &Path) {
        let Some(mut catalog) = read(&root.join("Local State"), MAX_STATE_BYTES)
            .and_then(|bytes| parse(browser, &bytes))
        else {
            // Do not keep obsolete identities after invalid or unavailable data.
            self.catalog = None;
            self.pictures.clear();
            return;
        };
        self.pictures
            .retain(|folder, _| catalog.profiles.iter().any(|p| p.folder == *folder));
        for profile in &mut catalog.profiles {
            if crate::runtime::stopping() {
                break;
            }
            let stamp = profile
                .picture_file
                .as_ref()
                .and_then(|file| picture_path(root, &profile.folder, file))
                .and_then(|path| {
                    let metadata = path.metadata().ok()?;
                    Some(Stamp {
                        path,
                        len: metadata.len(),
                        modified: metadata.modified().ok()?,
                    })
                });
            let Some(stamp) = stamp else {
                self.pictures.remove(&profile.folder);
                continue;
            };
            if self
                .pictures
                .get(&profile.folder)
                .is_none_or(|p| p.stamp != stamp)
            {
                let pixels = read(&stamp.path, MAX_PICTURE_BYTES).and_then(|bytes| decode(&bytes));
                self.pictures
                    .insert(profile.folder.clone(), Picture { stamp, pixels });
            }
            profile.pixels = self.pictures[&profile.folder].pixels.clone();
        }
        self.catalog = Some(catalog);
    }
}
pub(crate) fn start() -> (mpsc::SyncSender<Browsers>, mpsc::Receiver<Vec<Catalog>>) {
    let (tx, rx) = mpsc::sync_channel::<Browsers>(1);
    let (result_tx, result_rx) = mpsc::sync_channel(1);
    std::thread::spawn(move || {
        let mut previous: BTreeMap<Browser, CachedCatalog> = BTreeMap::new();
        while let Ok(browsers) = rx.recv() {
            if crate::runtime::stopping() {
                break;
            }
            previous.retain(|browser, _| browsers.contains(&Some(*browser)));
            let Some(home) = std::env::var_os("HOME") else {
                break;
            };
            for browser in browsers.into_iter().flatten() {
                let root = PathBuf::from(&home)
                    .join("Library/Application Support/Google")
                    .join(browser.directory());
                previous.entry(browser).or_default().refresh(browser, &root);
            }
            let _ = result_tx.try_send(
                previous
                    .values()
                    .filter_map(|p| p.catalog.clone())
                    .collect(),
            );
        }
    });
    (tx, result_rx)
}

#[cfg(test)]
mod tests {
    use super::*;
    // A synthetic, opaque 2 × 2 PNG. No account photo enters test fixtures.
    const PNG: &[u8] = &[
        137, 80, 78, 71, 13, 10, 26, 10, 0, 0, 0, 13, 73, 72, 68, 82, 0, 0, 0, 2, 0, 0, 0, 2, 8, 6,
        0, 0, 0, 114, 182, 13, 36, 0, 0, 0, 20, 73, 68, 65, 84, 120, 156, 99, 248, 207, 192, 240,
        31, 12, 129, 52, 16, 48, 252, 7, 0, 71, 202, 8, 248, 139, 78, 67, 133, 0, 0, 0, 0, 73, 69,
        78, 68, 174, 66, 96, 130,
    ];
    #[test]
    fn image_decode_has_fixed_size_and_rejects_large_or_invalid_sources() {
        let pixels = decode(PNG).unwrap();
        assert_eq!(pixels.len(), AVATAR_SIDE * AVATAR_SIDE * 4);
        assert!(pixels.chunks_exact(4).all(|p| p[3] == 255));
        assert!(decode(b"not a picture").is_none());
        assert!(decode(&vec![0; MAX_PICTURE_BYTES as usize + 1]).is_none());
        let mut invalid = PNG.to_vec();
        invalid[16..20].copy_from_slice(&1025u32.to_be_bytes());
        assert!(decode(&invalid).is_none());
        assert!(decode(&PNG[..24]).is_none());
    }
    #[test]
    fn unchanged_photos_share_pixels_and_removed_profiles_release_cache() {
        let root = std::env::temp_dir().join(format!("rowla-profile-cache-{}", std::process::id()));
        std::fs::create_dir_all(root.join("Default")).unwrap();
        let photo = root.join("Default/avatar.png");
        std::fs::write(&photo, PNG).unwrap();
        let state = root.join("Local State");
        std::fs::write(&state, br#"{"profile":{"info_cache":{"Default":{"name":"QA","use_gaia_picture":true,"gaia_picture_file_name":"avatar.png"}}}}"#).unwrap();
        let mut cached = CachedCatalog::default();
        cached.refresh(Browser::Stable, &root);
        let pixels = cached.catalog.as_ref().unwrap().profiles[0]
            .pixels
            .clone()
            .unwrap();
        cached.refresh(Browser::Stable, &root);
        assert!(Arc::ptr_eq(
            &pixels,
            cached.catalog.as_ref().unwrap().profiles[0]
                .pixels
                .as_ref()
                .unwrap()
        ));
        std::fs::write(&photo, b"invalid replacement").unwrap();
        cached.refresh(Browser::Stable, &root);
        assert!(
            cached.catalog.as_ref().unwrap().profiles[0]
                .pixels
                .is_none()
        );
        std::fs::write(&state, br#"{"profile":{"info_cache":{}}}"#).unwrap();
        cached.refresh(Browser::Stable, &root);
        assert!(cached.pictures.is_empty());
        std::fs::remove_file(state).unwrap();
        cached.refresh(Browser::Stable, &root);
        assert!(cached.catalog.is_none());
        std::fs::remove_dir_all(root).unwrap();
    }
}
