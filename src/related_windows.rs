//! Conservative grouping of owned window records. No native objects or images.
use crate::models::Window;
use std::collections::HashMap;

pub(crate) struct Group<'a> {
    pub main: &'a Window,
    pub children: Vec<&'a Window>,
}

fn same_frame(a: &Window, b: &Window) -> bool {
    a.width > 0.0
        && a.height > 0.0
        && [
            (a.x, b.x),
            (a.y, b.y),
            (a.width, b.width),
            (a.height, b.height),
        ]
        .iter()
        .all(|(a, b)| a.is_finite() && b.is_finite() && (a - b).abs() <= 1.0)
}

fn contains(parent: &Window, child: &Window) -> bool {
    let x = child.x + child.width / 2.0;
    let y = child.y + child.height / 2.0;
    parent.width > child.width
        && parent.height > child.height
        && x >= parent.x
        && x <= parent.x + parent.width
        && y >= parent.y
        && y <= parent.y + parent.height
}

pub(crate) fn groups<'a>(windows: &[&'a Window]) -> Vec<Group<'a>> {
    let mut apps: HashMap<i32, Vec<&Window>> = HashMap::new();
    for w in windows {
        apps.entry(w.pid).or_default().push(w);
    }
    let mut parents = HashMap::new();
    for w in windows {
        // Incomplete AX scans retain these owned records. Keep their known
        // relationships until a complete scan removes or updates them.
        let candidates: Vec<_> = apps[&w.pid]
            .iter()
            .copied()
            .filter(|p| p.id != w.id && !p.subordinate && !p.tabbed_hidden)
            .collect();
        let parent = if let Some(id) = w.parent_id {
            candidates.iter().find(|p| p.id == id).copied()
        } else if w.subordinate {
            // A utility panel often has AXApplication as its parent. Attach it
            // only to a sole main window or one unambiguous containing window.
            if candidates.len() == 1 {
                candidates.first().copied()
            } else {
                let mut contained = candidates.iter().copied().filter(|p| contains(p, w));
                let first = contained.next();
                first.filter(|_| contained.next().is_none())
            }
        } else if w.tabbed_hidden && !w.minimized && !w.hidden {
            // Geometry alone is insufficient: the visible owner must expose
            // a native tab bar. Ambiguous stacks remain separate.
            let mut matches = candidates
                .iter()
                .copied()
                .filter(|p| p.native_tabs && !p.minimized && !p.hidden && same_frame(p, w));
            let first = matches.next();
            first.filter(|_| matches.next().is_none())
        } else {
            None
        };
        if let Some(parent) = parent {
            parents.insert(w.id, parent.id);
        }
    }
    let positions: HashMap<_, _> = windows.iter().enumerate().map(|(i, w)| (w.id, i)).collect();
    let mut groups: Vec<_> = windows
        .iter()
        .filter(|w| !parents.contains_key(&w.id))
        .map(|w| Group {
            main: w,
            children: Vec::new(),
        })
        .collect();
    let roots: HashMap<_, _> = groups
        .iter()
        .enumerate()
        .map(|(i, g)| (g.main.id, i))
        .collect();
    for w in windows {
        if let Some(root) = parents.get(&w.id).and_then(|id| roots.get(id)) {
            groups[*root].children.push(w);
        }
    }
    groups.sort_by_key(|g| {
        g.children
            .iter()
            .map(|w| positions[&w.id])
            .chain(std::iter::once(positions[&g.main.id]))
            .min()
            .unwrap()
    });
    groups
}

#[cfg(test)]
mod tests {
    use super::*;
    fn window(id: u32) -> Window {
        Window {
            id,
            pid: 1,
            width: 600.0,
            height: 400.0,
            ..Window::default()
        }
    }
    fn ids(windows: &[Window]) -> Vec<(u32, Vec<u32>)> {
        groups(&windows.iter().collect::<Vec<_>>())
            .iter()
            .map(|g| (g.main.id, g.children.iter().map(|w| w.id).collect()))
            .collect()
    }
    #[test]
    fn utility_panels_do_not_consume_tiles_or_merge_unrelated_windows() {
        let main = window(1);
        let panel = Window {
            subordinate: true,
            width: 100.0,
            height: 80.0,
            ..window(2)
        };
        assert_eq!(ids(&[main.clone(), panel.clone()]), [(1, vec![2])]);
        assert_eq!(
            ids(&[main.clone(), window(3), panel.clone()]),
            [(1, vec![]), (3, vec![]), (2, vec![])]
        );
        // An explicit native relationship also applies to standard children.
        let explicit = Window {
            subordinate: false,
            parent_id: Some(3),
            ..panel
        };
        assert_eq!(
            ids(&[main, window(3), explicit]),
            [(1, vec![]), (3, vec![2])]
        );
    }
    #[test]
    fn tabs_need_native_evidence_matching_geometry_and_a_unique_owner() {
        let main = Window {
            native_tabs: true,
            ..window(1)
        };
        let tab = Window {
            tabbed_hidden: true,
            ..window(2)
        };
        assert_eq!(ids(&[tab.clone(), main.clone()]), [(1, vec![2])]);
        assert_eq!(
            ids(&[
                Window {
                    stale: true,
                    ..tab.clone()
                },
                Window {
                    stale: true,
                    ..main.clone()
                },
            ]),
            [(1, vec![2])]
        );
        assert_eq!(ids(&[window(1), tab.clone()]), [(1, vec![]), (2, vec![])]);
        assert_eq!(
            ids(&[
                main.clone(),
                Window {
                    x: 10.0,
                    ..tab.clone()
                }
            ]),
            [(1, vec![]), (2, vec![])]
        );
        assert_eq!(
            ids(&[
                main.clone(),
                Window {
                    minimized: true,
                    ..tab.clone()
                }
            ]),
            [(1, vec![]), (2, vec![])]
        );
        assert_eq!(
            ids(&[
                main.clone(),
                Window {
                    native_tabs: true,
                    ..window(3)
                },
                tab
            ]),
            [(1, vec![]), (3, vec![]), (2, vec![])]
        );
    }
    #[test]
    fn incomplete_scans_keep_computer_use_in_its_parent_tile() {
        let initial = vec![
            Window {
                pid: 2,
                ..window(10)
            },
            window(1),
            Window {
                pid: 3,
                ..window(20)
            },
            Window {
                subordinate: true,
                parent_id: Some(1),
                width: 200.0,
                height: 100.0,
                ..window(2)
            },
        ];
        let expected = [(10, vec![]), (1, vec![2]), (20, vec![])];
        for parent_id in [Some(1), None] {
            for cycle in 0..21 {
                let mut windows = initial.clone();
                windows[3].parent_id = parent_id;
                for w in windows.iter_mut().filter(|w| w.pid == 1) {
                    w.stale = cycle % 2 == 1;
                }
                assert_eq!(ids(&windows), expected, "scan {cycle}");
            }
        }
    }
    #[test]
    fn owners_must_be_present_and_from_the_same_process() {
        let child = Window {
            subordinate: true,
            parent_id: Some(1),
            ..window(2)
        };
        assert_eq!(
            ids(&[
                Window {
                    pid: 9,
                    ..window(1)
                },
                child.clone()
            ]),
            [(1, vec![]), (2, vec![])]
        );
        assert_eq!(ids(&[window(3), child]), [(3, vec![]), (2, vec![])]);
    }
}
