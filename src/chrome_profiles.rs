//! Local Chrome profile metadata. No browsing data, credentials, or native pointers.
use serde::{
    Deserialize,
    de::{self, MapAccess, Visitor},
};
use std::{
    collections::BTreeMap,
    fmt,
    path::{Component, Path, PathBuf},
    sync::Arc,
};

pub(crate) const MAX_PROFILES: usize = 32;
pub(crate) const MAX_STATE_BYTES: u64 = 2 * 1024 * 1024;
pub(crate) const AVATAR_SIDE: usize = 32;

#[derive(Clone, Copy, Debug, Eq, Ord, PartialEq, PartialOrd)]
pub(crate) enum Browser {
    Stable,
    Beta,
    Dev,
    Canary,
}
impl Browser {
    pub(crate) fn for_bundle(bundle: &str) -> Option<Self> {
        match bundle {
            "com.google.Chrome" => Some(Self::Stable),
            "com.google.Chrome.beta" => Some(Self::Beta),
            "com.google.Chrome.dev" => Some(Self::Dev),
            "com.google.Chrome.canary" => Some(Self::Canary),
            _ => None,
        }
    }
    pub(crate) fn directory(self) -> &'static str {
        match self {
            Self::Stable => "Chrome",
            Self::Beta => "Chrome Beta",
            Self::Dev => "Chrome Dev",
            Self::Canary => "Chrome Canary",
        }
    }
    #[cfg(test)]
    fn title(self) -> &'static str {
        match self {
            Self::Stable => "Google Chrome",
            Self::Beta => "Google Chrome Beta",
            Self::Dev => "Google Chrome Dev",
            Self::Canary => "Google Chrome Canary",
        }
    }
    fn separator(self) -> &'static str {
        match self {
            Self::Stable => " - Google Chrome - ",
            Self::Beta => " - Google Chrome Beta - ",
            Self::Dev => " - Google Chrome Dev - ",
            Self::Canary => " - Google Chrome Canary - ",
        }
    }
    fn suffix(self) -> &'static str {
        match self {
            Self::Stable => " - Google Chrome",
            Self::Beta => " - Google Chrome Beta",
            Self::Dev => " - Google Chrome Dev",
            Self::Canary => " - Google Chrome Canary",
        }
    }
}

#[derive(Clone, Debug, PartialEq)]
pub(crate) struct Profile {
    pub folder: String,
    pub name: String,
    pub color: u32,
    pub picture_file: Option<String>,
    /// Shared, owned BGRA pixels. Each image is exactly 32 × 32 pixels.
    pub pixels: Option<Arc<[u8]>>,
}
#[derive(Clone, Debug, PartialEq)]
pub(crate) struct Catalog {
    pub browser: Browser,
    pub profiles: Vec<Profile>,
}
impl Catalog {
    pub(crate) fn profile_for_title(&self, title: &str) -> Option<&Profile> {
        let name = title
            .rsplit_once(self.browser.separator())
            .map(|(_, name)| name);
        if let Some(name) = name {
            let mut matches = self.profiles.iter().filter(|p| p.name == name);
            let first = matches.next();
            return first.filter(|_| matches.next().is_none());
        }
        // Chrome omits the profile suffix when it has only one profile.
        // Incognito and Guest add their own suffix and cannot match this path.
        (self.profiles.len() == 1 && title.ends_with(self.browser.suffix()))
            .then(|| &self.profiles[0])
    }
}

#[derive(Default, Deserialize)]
#[serde(default)]
struct Entry {
    name: String,
    gaia_given_name: String,
    gaia_name: String,
    enterprise_label: String,
    is_using_default_name: bool,
    is_using_default_avatar: bool,
    use_gaia_picture: bool,
    gaia_picture_file_name: String,
    profile_highlight_color: Option<i64>,
}
impl Entry {
    fn local(&self) -> &str {
        if self.enterprise_label.is_empty() {
            &self.name
        } else {
            &self.enterprise_label
        }
    }
    fn gaia(&self) -> &str {
        if self.gaia_given_name.is_empty() {
            &self.gaia_name
        } else {
            &self.gaia_given_name
        }
    }
    fn display(&self, entries: &BTreeMap<String, Entry>) -> String {
        let local = self.local();
        let gaia = self.gaia();
        if gaia.is_empty() {
            return local.into();
        }
        let show_local = !local.eq_ignore_ascii_case(gaia)
            && (!self.is_using_default_name
                || !self.enterprise_label.is_empty()
                || entries.values().any(|other| {
                    !std::ptr::eq(self, other)
                        && other.gaia() == gaia
                        && (other.local().eq_ignore_ascii_case(gaia) || other.is_using_default_name)
                }));
        if show_local {
            format!("{gaia} ({local})")
        } else {
            gaia.into()
        }
    }
}
fn entries<'de, D: de::Deserializer<'de>>(decoder: D) -> Result<BTreeMap<String, Entry>, D::Error> {
    struct Bounded;
    impl<'de> Visitor<'de> for Bounded {
        type Value = BTreeMap<String, Entry>;
        fn expecting(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
            f.write_str("a bounded Chrome profile map")
        }
        fn visit_map<A: MapAccess<'de>>(self, mut map: A) -> Result<Self::Value, A::Error> {
            let mut result = BTreeMap::new();
            while let Some(key) = map.next_key::<String>()? {
                if result.len() >= MAX_PROFILES {
                    return Err(de::Error::custom("Chrome profile limit exceeded"));
                }
                if result.contains_key(&key) {
                    return Err(de::Error::custom("Duplicate Chrome profile directory"));
                }
                result.insert(key, map.next_value()?);
            }
            Ok(result)
        }
    }
    decoder.deserialize_map(Bounded)
}
#[derive(Default, Deserialize)]
struct ProfileState {
    #[serde(default, deserialize_with = "entries")]
    info_cache: BTreeMap<String, Entry>,
}
#[derive(Default, Deserialize)]
struct State {
    #[serde(default)]
    profile: ProfileState,
}

fn component(name: &str) -> bool {
    !name.is_empty()
        && name.len() <= 256
        && !name.contains(['/', '\\', '\0'])
        && matches!(
            Path::new(name).components().collect::<Vec<_>>().as_slice(),
            [Component::Normal(_)]
        )
}
pub(crate) fn picture_path(root: &Path, folder: &str, filename: &str) -> Option<PathBuf> {
    if !component(folder) || !component(filename) {
        return None;
    }
    let root = root.canonicalize().ok()?;
    let picture = root.join(folder).join(filename).canonicalize().ok()?;
    picture.starts_with(&root).then_some(picture)
}
pub(crate) fn parse(browser: Browser, bytes: &[u8]) -> Option<Catalog> {
    if bytes.len() as u64 > MAX_STATE_BYTES {
        return None;
    }
    let state: State = serde_json::from_slice(bytes).ok()?;
    // Reject invalid identities rather than infer the sole remaining profile.
    if state.profile.info_cache.iter().any(|(folder, p)| {
        !component(folder)
            || p.local().is_empty()
            || [p.local(), p.gaia()]
                .iter()
                .any(|n| n.len() > 512 || n.contains(['\n', '\r', '\0']))
    }) {
        return None;
    }
    let profiles = state
        .profile
        .info_cache
        .iter()
        .map(|(folder, p)| Profile {
            folder: folder.clone(),
            name: p.display(&state.profile.info_cache),
            color: p
                .profile_highlight_color
                .map_or(0xff4285f4, |color| color as u32 | 0xff000000),
            picture_file: ((p.use_gaia_picture || p.is_using_default_avatar)
                && component(&p.gaia_picture_file_name))
            .then(|| p.gaia_picture_file_name.clone()),
            pixels: None,
        })
        .collect();
    Some(Catalog { browser, profiles })
}

#[cfg(target_os = "macos")]
pub(crate) mod native;

#[cfg(test)]
mod tests {
    use super::*;
    fn catalog(info: serde_json::Value) -> Catalog {
        parse(
            Browser::Stable,
            &serde_json::to_vec(&serde_json::json!({"profile":{"info_cache":info}})).unwrap(),
        )
        .unwrap()
    }
    #[test]
    fn display_names_match_account_and_custom_profile_names() {
        let c = catalog(serde_json::json!({
            "Default":{"name":"Alex","gaia_given_name":"Alex"},
            "Profile 1":{"name":"Work","gaia_given_name":"Alex"},
            "Profile 2":{"name":"School","gaia_given_name":"ALEX"}
        }));
        assert_eq!(
            c.profiles
                .iter()
                .map(|p| p.name.as_str())
                .collect::<Vec<_>>(),
            ["Alex", "Alex (Work)", "ALEX (School)"]
        );
        assert_eq!(
            c.profile_for_title("Page - Google Chrome - Alex (Work)")
                .unwrap()
                .folder,
            "Profile 1"
        );
        assert!(
            c.profile_for_title("Page - Google Chrome - Unknown")
                .is_none()
        );
        assert!(
            c.profile_for_title("Alex (Work) in a page - Google Chrome")
                .is_none()
        );
        assert!(
            c.profile_for_title("Page - Google Chrome (Incognito)")
                .is_none()
        );
        assert!(
            c.profile_for_title("Page - Google Chrome (Guest)")
                .is_none()
        );
    }
    #[test]
    fn ambiguity_and_private_windows_do_not_get_an_account_badge() {
        let c = catalog(serde_json::json!({"Default":{"name":"Alex"},"Profile 1":{"name":"Alex"}}));
        assert!(c.profile_for_title("Page - Google Chrome - Alex").is_none());
        let c = catalog(serde_json::json!({"Default":{"name":"Alex"}}));
        assert!(c.profile_for_title("Page - Google Chrome").is_some());
        assert!(
            c.profile_for_title("Page - Google Chrome (Incognito)")
                .is_none()
        );
        assert!(c.profile_for_title("Dialog").is_none());
    }
    #[test]
    fn identity_paths_and_map_size_are_bounded() {
        assert!(!component("../other"));
        assert!(!component("/absolute"));
        assert!(!component(".."));
        assert!(!component("nested/file"));
        assert!(!component("Default/"));
        assert!(!component("Default/."));
        assert!(!component("..\\other"));
        assert!(component("Google Profile Picture.png"));
        let info: serde_json::Map<String, serde_json::Value> = (0..=MAX_PROFILES)
            .map(|i| (format!("Profile {i}"), serde_json::json!({"name":"Test"})))
            .collect();
        assert!(
            parse(
                Browser::Stable,
                &serde_json::to_vec(&serde_json::json!({"profile":{"info_cache":info}})).unwrap()
            )
            .is_none()
        );
        assert!(parse(Browser::Stable, &vec![b' '; MAX_STATE_BYTES as usize + 1]).is_none());
    }
    #[test]
    fn channels_use_their_own_catalog_and_title() {
        for (bundle, browser, directory) in [
            ("com.google.Chrome", Browser::Stable, "Chrome"),
            ("com.google.Chrome.beta", Browser::Beta, "Chrome Beta"),
            ("com.google.Chrome.dev", Browser::Dev, "Chrome Dev"),
            ("com.google.Chrome.canary", Browser::Canary, "Chrome Canary"),
        ] {
            assert_eq!(Browser::for_bundle(bundle), Some(browser));
            assert_eq!(browser.directory(), directory);
            let mut c = catalog(serde_json::json!({"Default":{"name":"Work"}}));
            c.browser = browser;
            assert!(
                c.profile_for_title(&format!("Page - {} - Work", browser.title()))
                    .is_some()
            );
        }
        assert_eq!(Browser::for_bundle("org.mozilla.firefox"), None);
        assert_eq!(AVATAR_SIDE * AVATAR_SIDE * 4, 4096);
    }
    #[test]
    fn picture_paths_cannot_escape_through_a_symlink() {
        let root = std::env::temp_dir().join(format!("rowla-profile-paths-{}", std::process::id()));
        let outside = root.with_extension("outside");
        std::fs::create_dir_all(root.join("Default")).unwrap();
        std::fs::write(root.join("Default/avatar.png"), b"photo").unwrap();
        std::fs::write(&outside, b"outside").unwrap();
        assert!(picture_path(&root, "Default", "avatar.png").is_some());
        #[cfg(unix)]
        {
            std::os::unix::fs::symlink(&outside, root.join("Default/link.png")).unwrap();
            assert!(picture_path(&root, "Default", "link.png").is_none());
        }
        std::fs::remove_dir_all(root).unwrap();
        std::fs::remove_file(outside).unwrap();
    }
}
