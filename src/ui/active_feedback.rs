//! Brief click feedback while the native focus report catches up.
use std::time::{Duration, Instant};

#[derive(Clone, Copy)]
pub(super) struct PendingFocus {
    pub id: u32,
    expires: Instant,
}
impl PendingFocus {
    pub fn new(id: u32, now: Instant) -> Self {
        Self {
            id,
            expires: now + Duration::from_secs(2),
        }
    }
    pub fn expired(self, now: Instant) -> bool {
        now >= self.expires
    }
    pub fn resolved(
        self,
        focused: Option<u32>,
        alive: bool,
        trusted: bool,
        failed: bool,
        now: Instant,
    ) -> bool {
        self.expired(now) || !alive || !trusted || failed || focused == Some(self.id)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn stale_reports_keep_feedback_until_focus_is_confirmed() {
        let now = Instant::now();
        let pending = PendingFocus::new(2, now);
        assert!(!pending.resolved(Some(1), true, true, false, now));
        assert!(!pending.resolved(None, true, true, false, now));
        assert!(pending.resolved(Some(2), true, true, false, now));
    }

    #[test]
    fn failed_closed_or_stalled_activation_cannot_leave_a_false_active_button() {
        let now = Instant::now();
        let pending = PendingFocus::new(2, now);
        assert!(pending.resolved(Some(1), true, true, true, now));
        assert!(pending.resolved(Some(1), false, true, false, now));
        assert!(pending.resolved(Some(1), true, false, false, now));
        assert!(pending.resolved(Some(1), true, true, false, now + Duration::from_secs(2)));
    }
}
