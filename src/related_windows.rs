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
    let mut apps = None;
    let mut parents = HashMap::new();
    for w in windows {
        if w.parent_id.is_none() && !w.subordinate && (!w.tabbed_hidden || w.minimized || w.hidden)
        {
            continue;
        }
        // Incomplete AX scans retain these owned records. Keep their known
        // relationships until a complete scan removes or updates them.
        let apps = apps.get_or_insert_with(|| {
            let mut apps: HashMap<i32, Vec<&Window>> = HashMap::new();
            for p in windows
                .iter()
                .copied()
                .filter(|p| !p.subordinate && !p.tabbed_hidden)
            {
                apps.entry(p.pid).or_default().push(p);
            }
            apps
        });
        let mut candidates = apps
            .get(&w.pid)
            .into_iter()
            .flatten()
            .copied()
            .filter(|p| p.id != w.id);
        let parent = if let Some(id) = w.parent_id {
            candidates.find(|p| p.id == id)
        } else if w.subordinate {
            // A utility panel often has AXApplication as its parent. Attach it
            // only to a sole main window or one unambiguous containing window.
            let mut unique = candidates.clone();
            let first = unique.next();
            if unique.next().is_none() {
                first
            } else {
                let mut contained = candidates.filter(|p| contains(p, w));
                let first = contained.next();
                first.filter(|_| contained.next().is_none())
            }
        } else {
            // Geometry alone is insufficient: the visible owner must expose
            // a native tab bar. Ambiguous stacks remain separate.
            let mut matches = candidates
                .filter(|p| p.native_tabs && !p.minimized && !p.hidden && same_frame(p, w));
            let first = matches.next();
            first.filter(|_| matches.next().is_none())
        };
        if let Some(parent) = parent {
            parents.insert(w.id, parent);
        }
    }
    drop(apps);
    if parents.is_empty() {
        return windows
            .iter()
            .map(|w| Group {
                main: w,
                children: Vec::new(),
            })
            .collect();
    }
    let root_count = windows.len() - parents.len();
    let mut groups: Vec<Group<'a>> = Vec::with_capacity(root_count);
    let mut roots = HashMap::with_capacity(root_count);
    for w in windows {
        let main = parents.get(&w.id).copied().unwrap_or(w);
        if parents.contains_key(&main.id) {
            continue;
        }
        // The first member fixes the group's position, including a hidden tab
        // that precedes its visible owner. No separate position map or sort.
        let root = *roots.entry(main.id).or_insert_with(|| {
            let index = groups.len();
            groups.push(Group {
                main,
                children: Vec::new(),
            });
            index
        });
        if w.id != main.id {
            groups[root].children.push(w);
        }
    }
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
    fn interleaved_groups_keep_the_position_of_their_first_member() {
        let child = |id, parent| Window {
            parent_id: Some(parent),
            ..window(id)
        };
        assert_eq!(
            ids(&[
                child(2, 1),
                window(10),
                child(4, 3),
                window(3),
                child(5, 1),
                window(1)
            ]),
            [(1, vec![2, 5]), (10, vec![]), (3, vec![4])]
        );
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
