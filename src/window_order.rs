//! Explicit, stable sorting. Focus history retains only living window IDs.
use crate::models::Window;
use std::{cmp::Reverse, collections::HashMap, time::Instant};

pub(crate) const MAX_APPS: usize = 128;

#[derive(Default)]
pub(crate) struct Activity {
    focused: Option<u32>,
    recent: HashMap<u32, (i32, Instant)>,
}
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

#[derive(PartialEq, Eq, Hash)]
enum AppKey<'a> {
    Bundle(&'a str),
    Process(i32),
}
fn app_key(w: &Window) -> AppKey<'_> {
    if w.bundle.is_empty() {
        AppKey::Process(w.pid)
    } else {
        AppKey::Bundle(&w.bundle)
    }
}

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
