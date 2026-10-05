//! Preview invalidation remains independent of pointer/tracking-area delivery.
use std::collections::HashSet;

pub(super) fn capture_invalid(
    source: Option<u32>,
    windows: &[crate::models::Window],
    permitted: bool,
) -> bool {
    source.is_some_and(|id| !permitted || !windows.iter().any(|w| w.id == id && !w.minimized))
}

pub(super) fn invalid_targets(
    hover: Option<u32>,
    displayed: Option<u32>,
    alive: &HashSet<u32>,
    permitted: bool,
) -> (bool, bool) {
    let hover_invalid = hover.is_some_and(|id| !permitted || !alive.contains(&id));
    let preview_invalid = displayed.is_some_and(|id| !permitted || !alive.contains(&id));
    (hover_invalid, preview_invalid)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn background_capture_stops_when_its_source_closes_or_becomes_minimized() {
        let mut windows = vec![crate::models::Window {
            id: 1,
            ..Default::default()
        }];
        assert!(!capture_invalid(Some(1), &windows, true));
        assert!(!capture_invalid(None, &windows, false));
        assert!(capture_invalid(Some(1), &windows, false));
        windows[0].minimized = true;
        assert!(capture_invalid(Some(1), &windows, true));
        windows.clear();
        assert!(capture_invalid(Some(1), &windows, true));
    }

    #[test]
    fn closing_window_after_pointer_enters_its_preview_clears_the_image() {
        let mut alive = HashSet::from([1]);
        assert_eq!(
            invalid_targets(Some(1), Some(1), &alive, true),
            (false, false)
        );
        // mouseExited clears hover while the pointer remains inside the popup.
        assert_eq!(invalid_targets(None, Some(1), &alive, true), (false, false));
        alive.remove(&1);
        assert_eq!(invalid_targets(None, Some(1), &alive, true), (false, true));
    }

    #[test]
    fn obsolete_preview_does_not_cancel_a_different_live_hover() {
        let alive = HashSet::from([2]);
        assert_eq!(
            invalid_targets(Some(2), Some(1), &alive, true),
            (false, true)
        );
    }

    #[test]
    fn closed_hover_cancels_work_without_requiring_a_displayed_image() {
        assert_eq!(
            invalid_targets(Some(1), None, &HashSet::new(), true),
            (true, false)
        );
    }

    #[test]
    fn live_and_absent_targets_need_no_cleanup() {
        let alive = HashSet::from([1, 2]);
        assert_eq!(
            invalid_targets(Some(1), Some(2), &alive, true),
            (false, false)
        );
        assert_eq!(
            invalid_targets(None, None, &HashSet::new(), true),
            (false, false)
        );
    }

    #[test]
    fn revoked_permission_clears_a_live_preview_after_pointer_exit() {
        let alive = HashSet::from([1]);
        assert_eq!(invalid_targets(None, Some(1), &alive, true), (false, false));
        assert_eq!(invalid_targets(None, Some(1), &alive, false), (false, true));
    }

    #[test]
    fn disabling_previews_cancels_pending_hover_without_waiting_for_window_closure() {
        let alive = HashSet::from([1]);
        assert_eq!(invalid_targets(Some(1), None, &alive, false), (true, false));
    }
}
