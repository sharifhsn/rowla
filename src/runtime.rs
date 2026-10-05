#[cfg(target_os = "macos")]
use std::sync::atomic::{AtomicBool, Ordering};
use std::{
    fs::{File, OpenOptions},
    path::Path,
    time::{Duration, Instant},
};

#[cfg(target_os = "macos")]
static STOPPING: AtomicBool = AtomicBool::new(false);
#[cfg(target_os = "macos")]
pub(crate) fn stop() {
    STOPPING.store(true, Ordering::Release);
}
#[cfg(target_os = "macos")]
pub(crate) fn stopping() -> bool {
    STOPPING.load(Ordering::Acquire)
}

/// Refresh one source at 1 Hz, but do not make a new hover wait for the previous
/// window's refresh deadline. Every attempt still obeys a global switch budget.
pub(crate) struct PreviewGate {
    last: Option<(u32, Instant)>,
}
impl PreviewGate {
    pub(crate) fn new() -> Self {
        Self { last: None }
    }
    pub(crate) fn allow(&mut self, source: u32, now: Instant) -> bool {
        if let Some((previous, at)) = self.last {
            let interval = if previous == source {
                Duration::from_secs(1)
            } else {
                Duration::from_millis(150)
            };
            if now.saturating_duration_since(at) < interval {
                return false;
            }
        }
        self.last = Some((source, now));
        true
    }
}
/// Keep the locked descriptor alive; never unlink the lock file (which would
/// let a second process lock a different inode).
pub(crate) struct InstanceGuard {
    _file: File,
}
impl InstanceGuard {
    pub(crate) fn acquire(path: &Path) -> Result<Self, String> {
        let mut options = OpenOptions::new();
        options.read(true).write(true).create(true).truncate(false);
        #[cfg(unix)]
        {
            use std::os::unix::fs::OpenOptionsExt;
            options.mode(0o600);
        }
        let file = options.open(path).map_err(|e| e.to_string())?;
        file.try_lock().map_err(|e| {
            format!("Rowla is already running or its instance lock is unavailable: {e}")
        })?;
        Ok(Self { _file: file })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn only_one_instance_owns_a_lock_and_exit_releases_it() {
        let path = std::env::temp_dir().join(format!("taskbar-lock-test-{}", std::process::id()));
        let first = InstanceGuard::acquire(&path).unwrap();
        assert!(InstanceGuard::acquire(&path).is_err());
        drop(first);
        drop(InstanceGuard::acquire(&path).unwrap());
        std::fs::remove_file(path).unwrap();
    }
    #[test]
    fn preview_switches_are_fast_but_attempts_and_same_source_refreshes_stay_bounded() {
        let start = Instant::now();
        let mut gate = PreviewGate::new();
        assert!(gate.allow(1, start));
        assert!(!gate.allow(2, start + Duration::from_millis(100)));
        assert!(gate.allow(2, start + Duration::from_millis(150)));
        assert!(!gate.allow(2, start + Duration::from_millis(900)));
        assert!(gate.allow(2, start + Duration::from_millis(1150)));
        assert!(!gate.allow(3, start + Duration::from_millis(1200)));
        assert!(gate.allow(3, start + Duration::from_millis(1300)));
    }
}
