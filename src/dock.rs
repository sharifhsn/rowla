//! Persist only the Dock keys we change, and restore them after a crash or clean quit.
use crate::config::Config;
use serde::{Deserialize, Serialize};
use std::{collections::BTreeMap, process::Command, sync::Mutex};
// Serialize apply/restore and reject queued worker changes after termination.
static SHUTTING_DOWN: Mutex<bool> = Mutex::new(false);
const APPLIED: [(&str, &str); 4] = [
    ("autohide", "true"),
    ("orientation", "left"),
    ("autohide-delay", "1000"),
    ("autohide-time-modifier", "0"),
];
#[derive(Serialize, Deserialize)]
struct Backup {
    values: BTreeMap<String, Option<String>>,
}
fn read(key: &str) -> Option<String> {
    let v = Command::new("/usr/bin/defaults")
        .args(["read", "com.apple.dock", key])
        .output()
        .ok()?;
    if v.status.success() {
        Some(String::from_utf8_lossy(&v.stdout).trim().into())
    } else {
        None
    }
}
fn write(key: &str, value: Option<&str>) -> Result<(), String> {
    let mut c = Command::new("/usr/bin/defaults");
    if let Some(v) = value {
        c.args(["write", "com.apple.dock", key]);
        if key == "orientation" {
            c.args(["-string", v]);
        } else if key == "autohide" {
            c.args([
                "-bool",
                if v == "1" || v == "true" {
                    "true"
                } else {
                    "false"
                },
            ]);
        } else {
            c.args(["-float", v]);
        }
    } else {
        c.args(["delete", "com.apple.dock", key]);
    }
    let o = c.output().map_err(|e| e.to_string())?;
    if o.status.success() || value.is_none() {
        Ok(())
    } else {
        Err(String::from_utf8_lossy(&o.stderr).into())
    }
}
fn restart() {
    let _ = Command::new("/usr/bin/killall").arg("Dock").output();
}
fn path() -> std::path::PathBuf {
    Config::directory().join("dock-backup.json")
}
pub fn apply(hidden: bool) -> Result<(), String> {
    let shutdown = SHUTTING_DOWN.lock().unwrap_or_else(|e| e.into_inner());
    if *shutdown {
        return Ok(());
    }
    if !hidden {
        return restore_locked();
    }
    if !path().exists() {
        let values = [
            "autohide",
            "orientation",
            "autohide-delay",
            "autohide-time-modifier",
        ]
        .into_iter()
        .map(|k| (k.into(), read(k)))
        .collect();
        std::fs::write(
            path(),
            serde_json::to_vec_pretty(&Backup { values }).unwrap(),
        )
        .map_err(|e| e.to_string())?;
    }
    for (k, v) in APPLIED {
        write(k, Some(v))?;
    }
    restart();
    Ok(())
}
pub fn shutdown_restore() -> Result<(), String> {
    let mut shutdown = SHUTTING_DOWN.lock().unwrap_or_else(|e| e.into_inner());
    *shutdown = true;
    restore_locked()
}
fn restore_locked() -> Result<(), String> {
    let Ok(data) = std::fs::read(path()) else {
        return Ok(());
    };
    let backup: Backup = serde_json::from_slice(&data).map_err(|e| e.to_string())?;
    for (k, v) in backup.values {
        // Preserve a Dock setting the user changed while this app was running.
        let current = read(&k);
        let expected = APPLIED
            .iter()
            .find(|(key, _)| *key == k)
            .map(|(_, value)| *value);
        let ours = match (k.as_str(), current.as_deref(), expected) {
            ("autohide", Some(now), Some(_)) => now == "1" || now == "true",
            ("orientation", Some(now), Some(value)) => now == value,
            (_, Some(now), Some(value)) => now.parse::<f64>().ok() == value.parse::<f64>().ok(),
            _ => false,
        };
        if ours {
            write(&k, v.as_deref())?;
        }
    }
    std::fs::remove_file(path()).map_err(|e| e.to_string())?;
    restart();
    Ok(())
}
