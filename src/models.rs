//! Owned values for integrations. No native pointers or captured pixels.
use std::collections::HashMap;

#[derive(Clone, Debug, Default, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct Window {
    pub id: u32,
    pub pid: i32,
    pub bundle: String,
    pub app: String,
    pub title: String,
    pub path: String,
    pub minimized: bool,
    pub hidden: bool,
    pub focused: bool,
    /// Dialogs and utility windows follow the application's main windows.
    #[serde(default)]
    pub subordinate: bool,
    pub fullscreen: bool,
    pub all_spaces: bool,
    pub on_space: bool,
    pub tabbed_hidden: bool,
    pub x: f64,
    pub y: f64,
    pub width: f64,
    pub height: f64,
    #[serde(default)]
    pub stale: bool,
}
#[derive(Clone, Debug, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct Application {
    pub bundle: String,
    pub name: String,
    pub path: String,
}
#[derive(Clone, Debug, Default, serde::Serialize, serde::Deserialize)]
pub struct Snapshot {
    pub windows: Vec<Window>,
    pub badges: HashMap<String, String>,
    pub trusted: bool,
    pub screen_allowed: bool,
    pub scan_ms: u128,
    #[serde(default)]
    pub control_error: Option<String>,
    #[serde(default)]
    pub capabilities: Capabilities,
    #[serde(default)]
    pub discovery: DiscoveryStatus,
}

#[derive(Clone, Debug, Default, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct Capabilities {
    pub private_window_ids: bool,
    pub private_spaces: bool,
}
#[derive(Clone, Debug, Default, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct DiscoveryStatus {
    pub pending_apps: usize,
    pub stale_apps: usize,
    pub observer_apps: usize,
    pub timeouts: u64,
    pub slices: u64,
    pub max_slice_ms: u128,
    pub incomplete: bool,
}
