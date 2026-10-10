//! Explicit, stable sorting. Focus history retains only living window IDs.
#[cfg(any(target_os = "macos", test))]
use crate::models::Window;
#[cfg(any(target_os = "macos", test))]
use std::{
    cmp::Reverse,
    collections::{HashMap, HashSet},
    time::Instant,
};

pub(crate) const MAX_APPS: usize = 128;

#[cfg(any(target_os = "macos", test))]
fn tab_owner(w: &Window) -> bool {
    w.native_tabs && w.on_space && (w.focused || !w.tabbed_hidden)
}
#[cfg(any(target_os = "macos", test))]
fn same_tabs(a: &Window, b: &Window) -> bool {
    a.pid == b.pid
        && a.bundle == b.bundle
        && a.tabs
            .iter()
            .any(|t| t.id != 0 && b.tabs.iter().any(|other| t.id == other.id))
}
#[cfg(any(target_os = "macos", test))]
pub(crate) fn reconcile(
    order: &mut Vec<u32>,
    spaces: &mut HashMap<u32, bool>,
    previous: &[Window],
    current: &[Window],
    reset_space_order: bool,
    alive: &HashSet<u32>,
) {
    let mut inherited = HashSet::new();
    for old in previous.iter().filter(|w| tab_owner(w)) {
        if current.iter().any(|w| w.id == old.id && tab_owner(w)) {
            continue;
        }
        let mut matches = current.iter().filter(|w| tab_owner(w) && same_tabs(old, w));
        let Some(next) = matches.next().filter(|_| matches.next().is_none()) else {
            continue;
        };
        if inherited.contains(&next.id) {
            continue;
        }
        let Some(slot) = order.iter().position(|id| *id == old.id) else {
            continue;
        };
        // AppKit changes the visible NSWindow when it selects a native tab.
        // Transfer its slot before closed IDs disappear. Shared worker-issued
        // control IDs prove the group, without title or geometry guesses.
        if let Some(other) = order.iter().position(|id| *id == next.id) {
            order.swap(slot, other);
        } else {
            order[slot] = next.id;
        }
        inherited.extend([old.id, next.id]);
    }
    order.retain(|id| alive.contains(id));
    spaces.retain(|id, _| alive.contains(id));
    let mut ordered: HashSet<_> = order.iter().copied().collect();
    for w in current {
        if reset_space_order
            && !inherited.contains(&w.id)
            && spaces.get(&w.id).is_some_and(|old| *old != w.on_space)
        {
            ordered.remove(&w.id);
        }
        spaces.insert(w.id, w.on_space);
    }
    order.retain(|id| ordered.contains(id));
    for w in current {
        if ordered.insert(w.id) {
            order.push(w.id);
        }
    }
}

#[cfg(any(target_os = "macos", test))]
#[derive(Default)]
pub(crate) struct Activity {
    focused: Option<u32>,
    recent: HashMap<u32, (i32, Instant)>,
}
#[cfg(any(target_os = "macos", test))]
impl Activity {
    pub(crate) fn observe(&mut self, windows: &[Window]) {
        let living: HashMap<_, _> = windows.iter().map(|w| (w.id, w.pid)).collect();
        self.recent
            .retain(|id, (pid, _)| living.get(id) == Some(pid));
        let focused = windows.iter().find(|w| w.focused);
        if let Some(w) = focused
            && (self.focused != Some(w.id) || !self.recent.contains_key(&w.id))
        {
            self.recent.insert(w.id, (w.pid, Instant::now()));
        }
        self.focused = focused.map(|w| w.id);
    }
    pub(crate) fn len(&self) -> usize {
        self.recent.len()
    }
}

#[cfg(any(target_os = "macos", test))]
#[derive(PartialEq, Eq, Hash)]
enum AppKey<'a> {
    Bundle(&'a str),
    Process(i32),
}
#[cfg(any(target_os = "macos", test))]
fn app_key(w: &Window) -> AppKey<'_> {
    if w.bundle.is_empty() {
        AppKey::Process(w.pid)
    } else {
        AppKey::Bundle(&w.bundle)
    }
}

#[cfg(any(target_os = "macos", test))]
pub(crate) fn sort(
    order: &mut Vec<u32>,
    windows: &[Window],
    configured: &[String],
    activity: &Activity,
) {
    let positions: HashMap<_, _> = order.iter().enumerate().map(|(i, id)| (*id, i)).collect();
    let mut tasks: Vec<_> = windows.iter().collect();
    // Discover unconfigured applications in current strip order, retaining their
    // position relative to one another rather than imposing a surprise alphabet.
    tasks.sort_by_key(|w| positions.get(&w.id).copied().unwrap_or(usize::MAX));
    let mut apps: HashMap<_, _> = configured
        .iter()
        .enumerate()
        .map(|(i, bundle)| (AppKey::Bundle(bundle.as_str()), i))
        .collect();
    for w in &tasks {
        let next = apps.len();
        apps.entry(app_key(w)).or_insert(next);
    }
    tasks.sort_by_cached_key(|w| {
        (
            apps[&app_key(w)],
            w.subordinate,
            Reverse(w.focused),
            Reverse(activity.recent.get(&w.id).map(|(_, time)| *time)),
            positions.get(&w.id).copied().unwrap_or(usize::MAX),
        )
    });
    order.clear();
    order.extend(tasks.iter().map(|w| w.id));
}

#[cfg(test)]
mod tests {
    use super::*;
    fn window(id: u32, app: &str, subordinate: bool) -> Window {
        Window {
            id,
            pid: id as i32,
            bundle: app.into(),
            subordinate,
            ..Window::default()
        }
    }
    fn tab_window(id: u32) -> Window {
        Window {
            pid: 42,
            on_space: true,
            native_tabs: true,
            tab_count: 2,
            focused: true,
            tabs: [101, 102]
                .into_iter()
                .map(|id| crate::models::WindowTab {
                    id,
                    resolved: true,
                    ..Default::default()
                })
                .collect(),
            ..window(id, "qa.native-tabs", false)
        }
    }
    fn update(
        order: &mut Vec<u32>,
        spaces: &mut HashMap<u32, bool>,
        old: &[Window],
        next: &[Window],
        reset: bool,
    ) {
        let alive = next.iter().map(|w| w.id).collect();
        reconcile(order, spaces, old, next, reset, &alive);
    }
    #[test]
    fn native_tab_replacements_keep_a_manual_slot_in_both_space_modes() {
        for reset in [false, true] {
            let mut order = vec![3, 10, 1];
            let mut spaces = HashMap::from([(3, false), (10, true), (1, false)]);
            let mut old = vec![window(1, "a", false), window(3, "b", false), tab_window(10)];
            for id in [11, 10, 11, 10] {
                let mut next = old.clone();
                next[2] = tab_window(id);
                update(&mut order, &mut spaces, &old, &next, reset);
                assert_eq!(order, [3, id, 1]);
                assert_eq!(spaces.len(), 3);
                old = next;
            }
        }
    }
    #[test]
    fn retained_inactive_tab_ids_swap_slots_without_duplicates() {
        let old = vec![
            tab_window(10),
            Window {
                on_space: false,
                tabbed_hidden: true,
                focused: false,
                ..tab_window(11)
            },
        ];
        let mut next = old.clone();
        next[0].on_space = false;
        next[0].tabbed_hidden = true;
        next[0].focused = false;
        next[1] = tab_window(11);
        let mut order = vec![10, 11];
        let mut spaces = HashMap::from([(10, true), (11, false)]);
        update(&mut order, &mut spaces, &old, &next, true);
        assert_eq!(order, [11, 10]);
        update(&mut order, &mut spaces, &next, &old, true);
        assert_eq!(order, [10, 11]);
    }
    #[test]
    fn a_tab_switch_after_restore_keeps_the_old_slot() {
        for (hidden, minimized) in [(true, false), (false, true)] {
            let old = vec![
                window(1, "a", false),
                Window {
                    hidden,
                    minimized,
                    focused: false,
                    ..tab_window(10)
                },
                window(3, "b", false),
            ];
            let next = vec![old[0].clone(), tab_window(11), old[2].clone()];
            let mut order = vec![3, 10, 1];
            update(&mut order, &mut HashMap::new(), &old, &next, true);
            assert_eq!(order, [3, 11, 1]);
        }
    }
    #[test]
    fn unrelated_and_ambiguous_tab_groups_do_not_inherit_positions() {
        let old = vec![window(1, "a", false), tab_window(10), window(3, "b", false)];
        for replacements in [
            vec![Window {
                pid: 99,
                ..tab_window(11)
            }],
            vec![Window {
                tabs: vec![],
                ..tab_window(11)
            }],
            vec![tab_window(11), tab_window(12)],
        ] {
            let mut next = vec![old[0].clone(), old[2].clone()];
            next.extend(replacements);
            let mut order = vec![1, 10, 3];
            update(&mut order, &mut HashMap::new(), &old, &next, false);
            assert_eq!(order[..2], [1, 3]);
        }
    }
    #[test]
    fn configured_app_precedes_recency_and_popups_follow_main_windows() {
        let mut windows = vec![
            window(1, "a", false),
            window(2, "a", false),
            window(3, "a", true),
            window(4, "b", false),
        ];
        let mut activity = Activity::default();
        for id in [1, 2, 3, 4] {
            for w in &mut windows {
                w.focused = w.id == id;
            }
            activity.observe(&windows);
        }
        let mut order = vec![3, 4, 1, 2];
        sort(&mut order, &windows, &["a".into(), "b".into()], &activity);
        assert_eq!(order, [2, 1, 3, 4]);
        let once = order.clone();
        sort(&mut order, &windows, &["a".into(), "b".into()], &activity);
        assert_eq!(order, once);
    }
    #[test]
    fn unknown_apps_and_unvisited_windows_keep_current_relative_order() {
        let windows = vec![
            window(1, "a", false),
            window(2, "b", false),
            window(3, "a", false),
            window(4, "c", false),
        ];
        let mut order = vec![4, 3, 2, 1];
        sort(&mut order, &windows, &["b".into()], &Activity::default());
        assert_eq!(order, [2, 4, 3, 1]);
    }
    #[test]
    fn history_drops_closed_windows_and_reused_ids_do_not_inherit_activity() {
        let mut windows = vec![window(1, "a", false), window(2, "a", false)];
        let mut activity = Activity::default();
        windows[0].focused = true;
        activity.observe(&windows);
        activity.observe(&windows);
        assert_eq!(activity.len(), 1);
        windows[0].focused = false;
        windows[0].pid = 20;
        activity.observe(&windows);
        assert_eq!(activity.len(), 0);
        windows[1].focused = true;
        activity.observe(&windows);
        windows.clear();
        activity.observe(&windows);
        assert_eq!(activity.len(), 0);
    }
    #[test]
    fn minimized_and_other_space_windows_are_sorted_without_changing_state() {
        let mut windows = vec![window(1, "a", true), window(2, "a", false)];
        windows[0].minimized = true;
        windows[1].on_space = false;
        let before = windows.clone();
        let mut order = vec![1, 2, 99];
        sort(&mut order, &windows, &[], &Activity::default());
        assert_eq!(order, [2, 1]);
        assert_eq!(windows, before);
    }

    #[test]
    #[ignore = "explicit configured window sort microbenchmark"]
    fn benchmark_window_sort() {
        let mut cases = Vec::new();
        for count in [8, 32, 64, 512] {
            let windows: Vec<_> = (1..=count)
                .map(|id| {
                    let mut w = window(id, &format!("qa.{}", id % 12), id % 7 == 0);
                    w.focused = id == count / 2;
                    w.minimized = id % 3 == 0;
                    w.on_space = id % 5 != 0;
                    w
                })
                .collect();
            let configured = vec!["qa.4".into(), "qa.1".into(), "qa.8".into()];
            let initial: Vec<_> = (1..=count).rev().collect();
            let mut activity = Activity::default();
            activity.observe(&windows);
            let mut expected = initial.clone();
            sort(&mut expected, &windows, &configured, &activity);
            let mut samples = Vec::new();
            for _ in 0..400 {
                let mut order = initial.clone();
                let start = Instant::now();
                sort(&mut order, &windows, &configured, &activity);
                samples.push(start.elapsed().as_secs_f64() * 1_000_000.0);
                assert_eq!(order, expected);
                std::hint::black_box(order);
            }
            samples.sort_by(f64::total_cmp);
            cases.push(serde_json::json!({"windows":count,"median_us":samples[200],"p95_us":samples[380],"order":expected}));
        }
        println!("{}", serde_json::json!({"window_sort":cases}));
    }
}
