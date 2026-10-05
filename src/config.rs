use serde::{Deserialize, Serialize};
use std::{
    collections::BTreeMap,
    fs,
    io::{Read, Write},
    path::{Path, PathBuf},
};

const MAX_CONFIG_BYTES: u64 = 1024 * 1024;
static WRITE_SEQUENCE: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(default)]
pub struct Config {
    pub start_at_login: bool,
    pub show_start: bool,
    pub show_sort: bool,
    pub app_order: Vec<String>,
    pub app_order_initialized: bool,
    pub start_action: String,
    pub scroll_down_hides: bool,
    pub scroll_up_hides: bool,
    pub middle_closes: bool,
    pub show_hidden: bool,
    pub show_badges: bool,
    pub show_all_spaces: bool,
    pub show_tabs: bool,
    pub group_by_app: bool,
    pub task_dragging: bool,
    pub pin_dragging: bool,
    pub click_hides_app: bool,
    pub all_displays: bool,
    pub main_only: bool,
    pub theme: String,
    pub center: bool,
    pub scale: f64,
    pub font_size: f64,
    pub thumbnail_scale: f64,
    pub thumbnail_font: f64,
    pub start_scale: f64,
    pub start_font: f64,
    pub transparency: f64,
    pub max_width: f64,
    pub show_titles: bool,
    pub thumbnail_titles: bool,
    pub hover_ms: u64,
    pub indicate_minimized: bool,
    pub indicate_hidden: bool,
    pub show_menubar: bool,
    pub thumbnails: bool,
    pub blacklist: Vec<String>,
    pub resize_overlap: bool,
    pub fully_hide_dock: bool,
    pub discord_hidden: bool,
    pub reset_space_order: bool,
    pub pins: Vec<Pin>,
    pub recent: Vec<String>,
    pub hidden_displays: Vec<u32>,
    pub update_policy: String,
    pub local_crash_reports: bool,
    /// Preserve extensions and settings from newer builds when saving.
    #[serde(flatten)]
    pub extensions: BTreeMap<String, serde_json::Value>,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Pin {
    #[serde(rename = "bundleIdentifier")]
    pub bundle: String,
    pub action: String,
}

impl Default for Config {
    fn default() -> Self {
        Self {
            start_at_login: false,
            show_start: true,
            show_sort: true,
            app_order: vec![],
            app_order_initialized: false,
            start_action: "menu".into(),
            scroll_down_hides: true,
            scroll_up_hides: false,
            middle_closes: true,
            show_hidden: true,
            show_badges: true,
            show_all_spaces: true,
            show_tabs: false,
            group_by_app: false,
            task_dragging: true,
            pin_dragging: true,
            click_hides_app: false,
            all_displays: false,
            main_only: false,
            theme: "auto".into(),
            center: false,
            scale: 100.0,
            font_size: 13.0,
            thumbnail_scale: 100.0,
            thumbnail_font: 13.0,
            start_scale: 100.0,
            start_font: 13.0,
            transparency: 50.0,
            max_width: 200.0,
            show_titles: true,
            thumbnail_titles: false,
            hover_ms: 0,
            indicate_minimized: true,
            indicate_hidden: true,
            show_menubar: true,
            thumbnails: true,
            blacklist: vec![],
            resize_overlap: false,
            fully_hide_dock: false,
            discord_hidden: true,
            reset_space_order: false,
            pins: vec![],
            recent: vec![],
            hidden_displays: vec![],
            update_policy: "manual".into(),
            local_crash_reports: true,
            extensions: BTreeMap::new(),
        }
    }
}

impl Config {
    pub fn directory() -> PathBuf {
        PathBuf::from(std::env::var_os("HOME").expect("HOME"))
            .join("Library/Application Support/Taskbar Rust")
    }
    pub fn path() -> PathBuf {
        Self::directory().join("config.json")
    }
    /// Missing preferences use defaults; malformed or unreadable preferences
    /// return an error and are never replaced implicitly.
    pub fn load() -> Result<Self, String> {
        Self::load_from(&Self::path())
    }
    pub fn load_from(path: &Path) -> Result<Self, String> {
        let file = match fs::File::open(path) {
            Ok(file) => file,
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(Self::default()),
            Err(e) => return Err(format!("Could not read {}: {e}", path.display())),
        };
        let mut bytes = Vec::new();
        file.take(MAX_CONFIG_BYTES + 1)
            .read_to_end(&mut bytes)
            .map_err(|e| e.to_string())?;
        if bytes.len() as u64 > MAX_CONFIG_BYTES {
            return Err("Preferences exceed the 1 MiB size limit; the file was preserved".into());
        }
        let mut c: Self = serde_json::from_slice(&bytes).map_err(|e| {
            format!(
                "Invalid preferences at {}: {e}. The file was preserved.",
                path.display()
            )
        })?;
        c.normalize();
        Ok(c)
    }
    pub fn save(&self) -> Result<(), String> {
        fs::create_dir_all(Self::directory()).map_err(|e| e.to_string())?;
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            fs::set_permissions(Self::directory(), fs::Permissions::from_mode(0o700))
                .map_err(|e| e.to_string())?;
        }
        self.save_to(&Self::path())
    }
    /// Write a complete, durable temporary file before atomically publishing it.
    pub fn save_to(&self, path: &Path) -> Result<(), String> {
        let mut normalized = self.clone();
        normalized.normalize();
        let bytes = serde_json::to_vec_pretty(&normalized).map_err(|e| e.to_string())?;
        if bytes.len() as u64 > MAX_CONFIG_BYTES {
            return Err("Preferences exceed the 1 MiB size limit".into());
        }
        // Public extension fields must not create duplicate reserved keys or
        // invalid JSON that the next launch cannot read.
        let _: Self = serde_json::from_slice(&bytes)
            .map_err(|e| format!("Invalid preference extensions: {e}"))?;
        let temp = path.with_extension(format!(
            "tmp.{}.{}.{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .map_err(|e| e.to_string())?
                .as_nanos(),
            WRITE_SEQUENCE.fetch_add(1, std::sync::atomic::Ordering::Relaxed)
        ));
        let mut created = false;
        let result = (|| {
            let mut options = fs::OpenOptions::new();
            options.write(true).create_new(true);
            #[cfg(unix)]
            {
                use std::os::unix::fs::OpenOptionsExt;
                options.mode(0o600);
            }
            let mut file = options.open(&temp).map_err(|e| e.to_string())?;
            created = true;
            file.write_all(&bytes).map_err(|e| e.to_string())?;
            file.sync_all().map_err(|e| e.to_string())?;
            fs::rename(&temp, path).map_err(|e| e.to_string())
        })();
        if result.is_err() && created {
            let _ = fs::remove_file(temp);
        }
        result
    }
    pub fn normalize(&mut self) {
        let defaults = Self::default();
        for (value, fallback) in [
            (&mut self.scale, defaults.scale),
            (&mut self.font_size, defaults.font_size),
            (&mut self.thumbnail_scale, defaults.thumbnail_scale),
            (&mut self.thumbnail_font, defaults.thumbnail_font),
            (&mut self.start_scale, defaults.start_scale),
            (&mut self.start_font, defaults.start_font),
            (&mut self.transparency, defaults.transparency),
            (&mut self.max_width, defaults.max_width),
        ] {
            if !value.is_finite() {
                *value = fallback;
            }
        }
        self.scale = self.scale.clamp(60.0, 200.0);
        self.font_size = self.font_size.clamp(9.0, 24.0);
        self.thumbnail_scale = self.thumbnail_scale.clamp(50.0, 200.0);
        self.thumbnail_font = self.thumbnail_font.clamp(9.0, 24.0);
        self.start_scale = self.start_scale.clamp(60.0, 160.0);
        self.start_font = self.start_font.clamp(9.0, 24.0);
        self.transparency = self.transparency.clamp(0.0, 95.0);
        self.max_width = self.max_width.clamp(60.0, 400.0);
        self.hover_ms = self.hover_ms.min(3000);
        self.pins.truncate(64);
        let mut seen = std::collections::HashSet::new();
        self.app_order.retain(|bundle| {
            !bundle.is_empty() && bundle.len() <= 512 && seen.insert(bundle.clone())
        });
        self.app_order.truncate(crate::window_order::MAX_APPS);
        self.app_order_initialized |= !self.app_order.is_empty();
        self.recent.truncate(12);
        self.blacklist.sort();
        self.blacklist.dedup();
        self.hidden_displays.sort();
        self.hidden_displays.dedup();
        if !["auto", "light", "dark"].contains(&self.theme.as_str()) {
            self.theme = "auto".into();
        }
        if !["manual", "check", "automatic"].contains(&self.update_policy.as_str()) {
            self.update_policy = "manual".into();
        }
    }
    pub fn remember(&mut self, bundle: &str) {
        self.recent.retain(|x| x != bundle);
        self.recent.insert(0, bundle.to_owned());
        self.recent.truncate(12);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    fn test_path(name: &str) -> PathBuf {
        std::env::temp_dir().join(format!("taskbar-config-{}-{name}.json", std::process::id()))
    }
    #[test]
    fn malformed_preferences_are_reported_without_destroying_original_bytes() {
        let path = test_path("malformed");
        let original = b"{\"pins\":[ broken json";
        fs::write(&path, original).unwrap();
        assert!(Config::load_from(&path).is_err());
        assert_eq!(fs::read(&path).unwrap(), original);
        fs::remove_file(path).unwrap();
    }
    #[test]
    fn parallel_atomic_writes_remain_parseable_and_preserve_extensions() {
        let path = test_path("concurrent");
        let handles: Vec<_> = (0..8)
            .map(|index| {
                let path = path.clone();
                std::thread::spawn(move || {
                    let mut c = Config::default();
                    c.extensions
                        .insert("future_setting".into(), serde_json::json!({"writer":index}));
                    for _ in 0..20 {
                        c.save_to(&path).unwrap();
                    }
                })
            })
            .collect();
        for handle in handles {
            handle.join().unwrap();
        }
        let c = Config::load_from(&path).unwrap();
        assert!(c.extensions.contains_key("future_setting"));
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            assert_eq!(
                fs::metadata(&path).unwrap().permissions().mode() & 0o777,
                0o600
            );
        }
        fs::remove_file(path).unwrap();
    }
    #[test]
    fn oversized_preferences_are_rejected_and_preserved() {
        let path = test_path("oversized");
        fs::write(&path, vec![b' '; MAX_CONFIG_BYTES as usize + 1]).unwrap();
        assert!(Config::load_from(&path).is_err());
        assert_eq!(fs::metadata(&path).unwrap().len(), MAX_CONFIG_BYTES + 1);
        fs::remove_file(path).unwrap();
    }
    #[test]
    fn integrations_cannot_save_non_finite_ui_dimensions() {
        let path = test_path("non-finite");
        let c = Config {
            scale: f64::NAN,
            max_width: f64::INFINITY,
            ..Config::default()
        };
        c.save_to(&path).unwrap();
        let restored = Config::load_from(&path).unwrap();
        assert_eq!(restored.scale, Config::default().scale);
        assert_eq!(restored.max_width, Config::default().max_width);
        fs::remove_file(path).unwrap();
    }
    #[test]
    fn extension_collisions_cannot_replace_a_valid_file() {
        let path = test_path("reserved-extension");
        let mut c = Config::default();
        c.save_to(&path).unwrap();
        let original = fs::read(&path).unwrap();
        c.extensions.insert("scale".into(), serde_json::json!(400));
        assert!(c.save_to(&path).is_err());
        assert_eq!(fs::read(&path).unwrap(), original);
        fs::remove_file(path).unwrap();
    }
    #[test]
    fn recent_launches_are_unique_and_bounded() {
        let mut c = Config::default();
        for n in 0..50 {
            c.remember(&format!("app.{n}"));
        }
        c.remember("app.49");
        assert_eq!(c.recent.len(), 12);
        assert_eq!(c.recent[0], "app.49");
        assert_eq!(c.recent.iter().filter(|x| *x == "app.49").count(), 1);
    }
    #[test]
    fn fresh_preferences_keep_login_and_window_resizing_opt_in() {
        let fresh = Config::default();
        assert!(!fresh.start_at_login && !fresh.resize_overlap && !fresh.fully_hide_dock);
        let saved: Config = serde_json::from_str(
            r#"{"start_at_login":true,"resize_overlap":true,"fully_hide_dock":true}"#,
        )
        .unwrap();
        assert!(saved.start_at_login && saved.resize_overlap && saved.fully_hide_dock);
    }
    #[test]
    fn legacy_config_adds_defaults_without_losing_pins() {
        let c:Config=serde_json::from_str(r#"{"pins":[{"bundleIdentifier":"com.apple.finder","action":"finderNewWindow"}],"thumbnails":false}"#).unwrap();
        assert!(!c.thumbnails);
        assert_eq!(c.pins[0].bundle, "com.apple.finder");
        assert_eq!(c.hover_ms, 0);
        assert_eq!(c.scale, 100.0);
        assert!(c.show_sort);
        assert!(c.app_order.is_empty());
    }
    #[test]
    fn application_order_is_unique_bounded_and_keeps_user_precedence() {
        let mut c = Config {
            app_order: vec!["app.b".into(), "".into(), "app.a".into(), "app.b".into()],
            ..Config::default()
        };
        c.app_order.extend((0..200).map(|n| format!("app.{n}")));
        c.normalize();
        assert_eq!(&c.app_order[..2], &["app.b", "app.a"]);
        assert_eq!(c.app_order.len(), crate::window_order::MAX_APPS);
        assert!(c.app_order_initialized);
        let path = test_path("app-order");
        c.save_to(&path).unwrap();
        assert_eq!(Config::load_from(&path).unwrap().app_order, c.app_order);
        c.app_order.clear();
        c.save_to(&path).unwrap();
        let empty = Config::load_from(&path).unwrap();
        assert!(empty.app_order.is_empty() && empty.app_order_initialized);
        fs::remove_file(path).unwrap();
    }
}
