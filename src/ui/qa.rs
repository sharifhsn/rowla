//! Bounded interaction timings and a hidden AppKit hit-test/render workload.
//! Synthetic records never control real windows or save their titles/pixels.
use super::*;
use std::collections::VecDeque;

#[derive(Default)]
pub(super) struct Timing {
    count: u64,
    max_ms: f64,
    samples: VecDeque<f64>,
}
impl Timing {
    pub(super) fn record(&mut self, elapsed: Duration) {
        let ms = elapsed.as_secs_f64() * 1000.0;
        self.count += 1;
        self.max_ms = self.max_ms.max(ms);
        if self.samples.len() == 256 {
            self.samples.pop_front();
        }
        self.samples.push_back(ms);
    }
    pub(super) fn json(&self) -> serde_json::Value {
        let mut samples: Vec<f64> = self.samples.iter().copied().collect();
        samples.sort_by(f64::total_cmp);
        let percentile = |fraction: f64| {
            samples
                .get(((samples.len().saturating_sub(1)) as f64 * fraction).ceil() as usize)
                .copied()
        };
        serde_json::json!({"count":self.count,"retained_samples":samples.len(),"median_ms":percentile(0.5),"p95_ms":percentile(0.95),"max_ms":self.max_ms})
    }
}
#[derive(Default)]
pub(super) struct Performance {
    pub buttons_created: u64,
    pub commands: Cell<u64>,
    pub render: Timing,
    pub hover: Timing,
    pub cached_hover: Timing,
    pub click_feedback: Timing,
    pub capture: Timing,
    pub sort: Timing,
    pub capture_cancelled: u64,
}
impl Performance {
    pub(super) fn json(&self) -> serde_json::Value {
        serde_json::json!({"buttons_created":self.buttons_created,"window_commands":self.commands.get(),"render":self.render.json(),"hover_to_preview":self.hover.json(),"cached_hover_to_preview":self.cached_hover.json(),"click_to_active_feedback":self.click_feedback.json(),"capture":self.capture.json(),"capture_cancelled":self.capture_cancelled,"sort":self.sort.json()})
    }
}

pub(super) fn run(state: &Shared, count: usize) -> bool {
    eprintln!("Native UI check: fixture handlers");
    {
        let mut s = state.borrow_mut();
        s.config.pins.clear();
        s.config.blacklist.clear();
        s.config.show_start = true;
        s.config.show_titles = true;
        s.config.show_badges = true;
        s.config.all_displays = true;
        s.config.hidden_displays.clear();
        s.config.main_only = false;
        s.config.center = false;
        s.config.resize_overlap = false;
        s.snapshot.trusted = true;
        s.snapshot
            .badges
            .insert("com.apple.TextEdit".into(), "1".into());
    }
    let mut hits = 0;
    let mut misses = 0;
    let mut active_feedback_checks = 0;
    let mut active_feedback_failures = 0;
    let mut hover_close_hits = 0;
    let mut cases = Vec::new();
    let mut miss_details = Vec::new();
    for windows in [8usize, 32, 64] {
        eprintln!("Native UI check: {windows} fixture buttons");
        {
            let mut s = state.borrow_mut();
            s.snapshot.windows = (1..=windows)
                .map(|id| Window {
                    id: id as u32,
                    bundle: "com.apple.TextEdit".into(),
                    app: "Taskbar QA".into(),
                    title: format!(
                        "Taskbar QA {id} — a long window title that must stay on one line"
                    ),
                    on_space: true,
                    x: 0.0,
                    y: 10.0,
                    width: 300.0,
                    height: 200.0,
                    ..Window::default()
                })
                .collect();
            s.order = (1..=windows as u32).collect();
            s.pending_focus = None;
            s.performance = Performance::default();
        }
        // Warm the icon/AppKit caches before timing redraws.
        autoreleasepool(|_| render(state));
        state.borrow_mut().performance.render = Timing::default();
        for cycle in 0..count {
            let mut s = state.borrow_mut();
            for (i, w) in s.snapshot.windows.iter_mut().enumerate() {
                w.focused = i == cycle % windows;
                w.x = (cycle % 2) as f64;
            }
            drop(s);
            autoreleasepool(|_| render(state));
        }
        let (root, panel, buttons) = {
            let s = state.borrow();
            let bar = &s.bars[0];
            let mut buttons: Vec<_> = bar
                .buttons
                .values()
                .filter(|b| matches!(b.ivars().action, Action::Window(_)))
                .cloned()
                .collect();
            buttons.sort_by(|a, b| a.frame().origin.x.total_cmp(&b.frame().origin.x));
            (bar.root.clone(), bar.panel.clone(), buttons)
        };
        let single_line_buttons = buttons
            .iter()
            .filter(|b| b.usesSingleLineMode() && b.cell().is_some_and(|cell| !cell.wraps()))
            .count();
        let truncated_buttons = buttons
            .iter()
            .filter(|b| b.lineBreakMode() == NSLineBreakMode::ByTruncatingTail)
            .count();
        let mut visible_buttons = 0;
        for button in &buttons {
            let r = button.frame();
            state.borrow().bars[0].tasks.scrollRectToVisible(r);
            let origin = button
                .convertRect_toView(button.bounds(), Some(&root))
                .origin;
            if origin.x < 0.0 || origin.x + r.size.width > root.bounds().size.width {
                continue;
            }
            visible_buttons += 1;
            // Corners/edges, centre/icon/title, and the notification-dot location.
            for (x, y) in [
                (0.5, 0.5),
                (r.size.width - 0.5, 0.5),
                (0.5, root.bounds().size.height - 0.5),
                (r.size.width - 0.5, root.bounds().size.height - 0.5),
                (r.size.width / 2.0, root.bounds().size.height / 2.0),
                (r.size.width - 6.0, root.bounds().size.height - 6.0),
            ] {
                let point = button.convertPoint_toView(NSPoint::new(x, y), Some(&root));
                let hit = root.hitTest(point);
                if hit
                    .as_deref()
                    .is_some_and(|v| std::ptr::eq(v, &**button as &NSView))
                {
                    hits += 1;
                    let point = button.convertPoint_toView(NSPoint::new(x, y), None);
                    let event = |kind| {
                        NSEvent::mouseEventWithType_location_modifierFlags_timestamp_windowNumber_context_eventNumber_clickCount_pressure(kind,point,NSEventModifierFlags::empty(),0.0,panel.windowNumber(),None,0,1,1.0).unwrap()
                    };
                    let down = event(NSEventType::LeftMouseDown);
                    let up = event(NSEventType::LeftMouseUp);
                    let Action::Window(id) = button.ivars().action else {
                        unreachable!()
                    };
                    let screen = panel.convertPointToScreen(point);
                    if hover_close::tile_at(&state.borrow().bars[0], screen) == Some(id) {
                        hover_close_hits += 1;
                    }
                    let activating = state
                        .borrow()
                        .snapshot
                        .windows
                        .iter()
                        .any(|w| w.id == id && !w.focused);
                    unsafe {
                        let _: () = msg_send![&**button,mouseDown:&*down];
                        let _: () = msg_send![&**button,mouseUp:&*up];
                    }
                    if activating {
                        active_feedback_checks += 1;
                        if state.borrow().focused_id() != Some(id) {
                            active_feedback_failures += 1;
                        }
                    }
                } else {
                    misses += 1;
                    if miss_details.len() < 8 {
                        miss_details.push(serde_json::json!({"point":format!("{point:?}"),"button_frame":format!("{:?}",button.frame()),"button_bounds":format!("{:?}",button.bounds()),"hit":hit.as_deref().map(|v|v.class().name().to_string_lossy().to_string()),"clip_frame":format!("{:?}",state.borrow().bars[0].scroll.contentView().frame()),"clip_bounds":format!("{:?}",state.borrow().bars[0].scroll.contentView().bounds())}));
                    }
                }
            }
        }
        let s = state.borrow();
        cases.push(serde_json::json!({"windows":windows,"visible_buttons":visible_buttons,"buttons":buttons.len(),"single_line_buttons":single_line_buttons,"truncated_buttons":truncated_buttons,"panel_height":panel.frame().size.height,"expected_panel_height":32.0*s.config.scale/100.0,"panel_hidden":!panel.isVisible(),"performance":s.performance.json()}));
    }
    eprintln!("Native UI check: interaction regressions");
    let presentation_checks = check_presentation_updates(state);
    let cursor_and_clicks = check_cursor_and_clicks(state);
    let focus_clicks = check_focus_clicks(state);
    let preview_frame_checks = check_preview_frame(state);
    let minimized_preview = check_minimized_preview(state);
    let close_compact = state.borrow().preview.as_ref().is_some_and(|preview| {
        let close = &preview.close;
        close.frame().size == NSSize::new(18.0, 18.0)
            && !close.isBordered()
            && close.font().is_some_and(|font| font.pointSize() == 12.0)
    });
    let sort_checks = check_sort(state);
    let hover_close_ignores_controls = {
        let s = state.borrow();
        let bar = &s.bars[0];
        bar.buttons
            .values()
            .filter(|b| !matches!(b.ivars().action, Action::Window(_)))
            .all(|b| {
                let p = b.convertPoint_toView(
                    NSPoint::new(b.bounds().size.width / 2.0, b.bounds().size.height / 2.0),
                    None,
                );
                hover_close::tile_at(bar, bar.panel.convertPointToScreen(p)).is_none()
            })
    };
    let pin_checks = check_pins(state);
    let start_search = check_start_search(state);
    let snapshot_checks = check_snapshot_updates(state, count);
    eprintln!("Native UI check: system features");
    let system_features = check_system_features(state);
    let related_windows = check_related_windows(state);
    let chrome_profiles = check_chrome_profiles(state);
    let native_tab_order = check_native_tab_order(state);
    let passed = misses == 0
        && hits == 624
        && hover_close_hits == hits
        && hover_close_ignores_controls
        && active_feedback_checks > 0
        && active_feedback_failures == 0
        && close_compact
        && presentation_checks["passed"] == true
        && cursor_and_clicks["passed"] == true
        && focus_clicks["passed"] == true
        && preview_frame_checks["passed"] == true
        && minimized_preview["passed"] == true
        && sort_checks["passed"] == true
        && pin_checks["passed"] == true
        && start_search["passed"] == true
        && snapshot_checks["passed"] == true
        && system_features["passed"] == true
        && related_windows["passed"] == true
        && chrome_profiles["passed"] == true
        && native_tab_order["passed"] == true
        && cases.iter().all(|c| {
            c["windows"] == c["visible_buttons"]
                && c["windows"] == c["single_line_buttons"]
                && c["windows"] == c["truncated_buttons"]
                && c["panel_height"] == c["expected_panel_height"]
                && c["performance"]["window_commands"].as_u64()
                    == Some(c["windows"].as_u64().unwrap() * 6)
        });
    println!(
        "{}",
        serde_json::json!({"completed":true,"passed":passed,"cycles_per_case":count,"hit_successes":hits,"hit_misses":misses,"hover_close_hits":hover_close_hits,"hover_close_ignores_controls":hover_close_ignores_controls,"active_feedback_checks":active_feedback_checks,"active_feedback_failures":active_feedback_failures,"presentation":presentation_checks,"preview_close_compact":close_compact,"preview_frame":preview_frame_checks,"minimized_preview":minimized_preview,"cursor_and_clicks":cursor_and_clicks,"focus_clicks":focus_clicks,"sort":sort_checks,"pins":pin_checks,"start_search":start_search,"snapshot_updates":snapshot_checks,"system_features":system_features,"related_windows":related_windows,"chrome_profiles":chrome_profiles,"native_tab_order":native_tab_order,"miss_details":miss_details,"cases":cases})
    );
    passed
}

fn check_native_tab_order(state: &Shared) -> serde_json::Value {
    let saved = {
        let s = state.borrow();
        (
            s.config.clone(),
            s.snapshot.clone(),
            s.order.clone(),
            s.known_spaces.clone(),
        )
    };
    let owner = |id| Window {
        id,
        pid: 42,
        app: "Native Tab QA".into(),
        bundle: "qa.native-tabs".into(),
        on_space: true,
        focused: true,
        native_tabs: true,
        tab_count: 2,
        width: 600.0,
        height: 400.0,
        title: format!("Terminal {id}"),
        tabs: [101, 102]
            .into_iter()
            .map(|token| crate::models::WindowTab {
                id: token,
                title: format!("Tab {token}"),
                selected: token == if id == 10 { 101 } else { 102 },
                resolved: true,
            })
            .collect(),
        ..Window::default()
    };
    let neighbor = |id| Window {
        id,
        pid: id as i32,
        on_space: true,
        app: "Neighbor QA".into(),
        bundle: format!("qa.neighbor.{id}"),
        width: 600.0,
        height: 400.0,
        ..Window::default()
    };
    let snapshot = |id| Snapshot {
        trusted: true,
        screen_allowed: true,
        windows: vec![neighbor(1), owner(id), neighbor(3)],
        ..Snapshot::default()
    };
    let mut stable = true;
    let mut feedback = true;
    for reset in [false, true] {
        {
            let mut s = state.borrow_mut();
            s.config.compact_related_windows = true;
            s.config.show_tabs = true;
            s.config.main_only = false;
            s.config.all_displays = true;
            s.config.blacklist.clear();
            s.config.hidden_displays.clear();
            s.hidden_now.clear();
            s.config.reset_space_order = reset;
            s.snapshot = snapshot(10);
            s.order = vec![3, 10, 1];
            s.known_spaces = HashMap::from([(3, true), (10, true), (1, true)]);
            s.pending_focus = None;
            s.pending_tab = None;
        }
        render(state);
        let frames: Vec<_> = [3, 10, 1]
            .iter()
            .map(|id| state.borrow().bars[0].buttons[&Action::Window(*id)].frame())
            .collect();
        for cycle in 0..12 {
            let old = if cycle % 2 == 0 { 10 } else { 11 };
            let next = if old == 10 { 11 } else { 10 };
            let token = if next == 10 { 101 } else { 102 };
            related::select_tab(state, old, token);
            feedback &= state.borrow().bars[0].buttons[&Action::Tab(old, token)]
                .ivars()
                .window_style
                .get()
                .is_some_and(|s| s.focused);
            apply_snapshot(&mut state.borrow_mut(), snapshot(next));
            render(state);
            let s = state.borrow();
            stable &= s.order == [3, next, 1]
                && s.known_spaces.len() == 3
                && [3, next, 1]
                    .iter()
                    .zip(&frames)
                    .all(|(id, frame)| s.bars[0].buttons[&Action::Window(*id)].frame() == *frame)
                && s.bars[0].buttons[&Action::Tab(next, token)]
                    .ivars()
                    .window_style
                    .get()
                    .is_some_and(|s| s.focused);
        }
    }
    {
        let mut s = state.borrow_mut();
        (s.config, s.snapshot, s.order, s.known_spaces) = saved;
        s.pending_focus = None;
        s.pending_tab = None;
    }
    render(state);
    serde_json::json!({"passed":stable&&feedback,"transitions":24,"manual_slot_and_neighbor_frames":stable,"instant_tab_feedback":feedback})
}

fn check_chrome_profiles(state: &Shared) -> serde_json::Value {
    use crate::chrome_profiles::{AVATAR_SIDE, Browser, Catalog, Profile};
    let saved = {
        let mut s = state.borrow_mut();
        (
            s.config.clone(),
            s.snapshot.clone(),
            s.order.clone(),
            std::mem::take(&mut s.profile_badges),
        )
    };
    let profile = |folder: &str, name: &str, color: u32, picture: bool| Profile {
        folder: folder.into(),
        name: name.into(),
        color,
        picture_file: None,
        pixels: picture.then(|| {
            [color as u8, (color >> 8) as u8, (color >> 16) as u8, 255]
                .repeat(AVATAR_SIDE * AVATAR_SIDE)
                .into()
        }),
    };
    {
        let mut s = state.borrow_mut();
        s.config.chrome_profile_badges = true;
        s.config.compact_related_windows = false;
        s.config.show_titles = true;
        s.config.scale = 100.0;
        s.config.max_width = 235.0;
        s.config.main_only = false;
        s.config.blacklist.clear();
        s.config.hidden_displays.clear();
        s.hidden_now.clear();
        s.config.show_tabs = true;
        s.profile_badges.install(vec![Catalog {
            browser: Browser::Stable,
            profiles: vec![
                profile("Default", "Alex", 0xffc44862, true),
                profile("Profile 1", "Alex (Work)", 0xff3478bc, true),
                profile("Profile 2", "School", 0xff56843d, false),
            ],
        }]);
        s.snapshot.windows = (1..=6)
            .map(|id| Window {
                id,
                pid: 1,
                app: "Google Chrome".into(),
                bundle: "com.google.Chrome".into(),
                path: "/Applications/Google Chrome.app".into(),
                title: match id {
                    1 => "Home - Google Chrome - Alex",
                    2 => "Work - Google Chrome - Alex (Work)",
                    3 => "Minimized - Google Chrome - Alex",
                    4 => "School - Google Chrome - School",
                    5 => "Private - Google Chrome (Incognito)",
                    _ => "Unknown - Google Chrome - Other",
                }
                .into(),
                minimized: id == 3,
                on_space: true,
                width: 600.0,
                height: 400.0,
                ..Window::default()
            })
            .collect();
        s.order = (1..=6).collect();
    }
    render(state);
    let buttons = || {
        let s = state.borrow();
        (1..=6)
            .map(|id| s.bars[0].buttons[&Action::Window(id)].clone())
            .collect::<Vec<_>>()
    };
    let bs = buttons();
    let image = |b: &ActionButton| {
        b.ivars()
            .profile_badge
            .borrow()
            .as_ref()
            .map(|i| std::ptr::from_ref(&**i))
    };
    let mapped = bs[..4].iter().all(|b| image(b).is_some())
        && bs[4..].iter().all(|b| image(b).is_none())
        && image(&bs[0]) == image(&bs[2])
        && image(&bs[0]) != image(&bs[1])
        && bs[1]
            .toolTip()
            .unwrap()
            .to_string()
            .contains("Chrome profile: Alex (Work)");
    let created = state.borrow().performance.buttons_created;
    for _ in 0..100 {
        render(state);
    }
    let reused = created == state.borrow().performance.buttons_created
        && buttons()
            .iter()
            .zip(&bs)
            .all(|(a, b)| std::ptr::eq(&**a, &**b) && image(a) == image(b));
    let bounds_ok = |b: &ActionButton| {
        chrome_profiles::frame(b).is_some_and(|f| {
            f.origin.x >= 0.0
                && f.origin.y >= 0.0
                && f.origin.x + f.size.width <= b.bounds().size.width
                && f.origin.y + f.size.height <= b.bounds().size.height
                && f.size.width <= 12.0
        })
    };
    let bounds = bs[..4].iter().all(|b| bounds_ok(b));
    let root = state.borrow().bars[0].root.clone();
    let panel = state.borrow().bars[0].panel.clone();
    let f = chrome_profiles::frame(&bs[0]).unwrap();
    let point = bs[0].convertPoint_toView(
        NSPoint::new(
            f.origin.x + f.size.width / 2.0,
            f.origin.y + f.size.height / 2.0,
        ),
        None,
    );
    let hit = root
        .hitTest(root.convertPoint_fromView(point, None))
        .is_some_and(|v| std::ptr::eq(&*v, &*bs[0] as &NSView));
    let close_target =
        hover_close::tile_at(&state.borrow().bars[0], panel.convertPointToScreen(point)) == Some(1);
    let screenshot = render_fixture_view(&root, "chrome-profile-badges.png");
    state.borrow_mut().config.show_titles = false;
    state.borrow_mut().config.scale = 60.0;
    render(state);
    let scaled_height = state.borrow().bars[0].panel.frame().size.height;
    // AppKit rounds window frames to display pixels at fractional scales.
    let narrow = buttons()[..4].iter().all(|b| bounds_ok(b)) && (scaled_height - 19.2).abs() < 1.0;
    state.borrow_mut().config.chrome_profile_badges = false;
    render(state);
    let disabled = buttons().iter().all(|b| image(b).is_none());
    {
        let mut s = state.borrow_mut();
        (s.config, s.snapshot, s.order, s.profile_badges) = saved;
    }
    render(state);
    serde_json::json!({"passed":mapped&&reused&&bounds&&hit&&close_target&&screenshot&&narrow&&disabled,
        "profile_mapping":mapped,"shared_images_and_controls":reused,"badge_bounds":bounds,
        "badge_hit_target":hit,"hover_close_target":close_target,"synthetic_screenshot":screenshot,
        "icon_only_scaled_bounds":narrow,"scaled_panel_height":scaled_height,"disabled_clears_images":disabled})
}

fn check_related_windows(state: &Shared) -> serde_json::Value {
    let saved = {
        let s = state.borrow();
        (
            s.config.clone(),
            s.snapshot.clone(),
            s.order.clone(),
            s.pending_focus,
        )
    };
    {
        let mut s = state.borrow_mut();
        s.config.compact_related_windows = true;
        s.config.blacklist.clear();
        s.config.hidden_displays.clear();
        s.config.main_only = false;
        s.hidden_now.clear();
        s.config.show_tabs = true;
        s.config.show_titles = true;
        s.config.max_width = 200.0;
        s.config.all_displays = true;
        s.config.scale = 100.0;
        s.pending_focus = None;
        s.snapshot.windows = (1..=9)
            .map(|id| Window {
                id,
                pid: 1,
                app: "Related QA".into(),
                bundle: "com.apple.TextEdit".into(),
                title: if id == 1 {
                    "Chat window".into()
                } else {
                    format!("Computer Use {id}")
                },
                subordinate: id > 1,
                parent_id: (id > 1).then_some(1),
                on_space: true,
                width: if id == 1 { 600.0 } else { 200.0 },
                height: if id == 1 { 400.0 } else { 100.0 },
                ..Window::default()
            })
            .collect();
        s.order = (1..=9).collect();
    }
    render(state);
    let (parent, bubble, overflow, panel, root, created) = {
        let s = state.borrow();
        let b = &s.bars[0];
        (
            b.buttons[&Action::Window(1)].clone(),
            b.buttons[&Action::Bubble(2)].clone(),
            b.buttons[&Action::RelatedMore(1)].clone(),
            b.panel.clone(),
            b.root.clone(),
            s.performance.buttons_created,
        )
    };
    let compact = state.borrow().bars[0]
        .buttons
        .keys()
        .filter(|a| matches!(a, Action::Window(_)))
        .count()
        == 1
        && panel.frame().size.height == 32.0
        && bubble.frame().size == NSSize::new(18.0, 18.0)
        && overflow.title().to_string() == "+6"
        && parent.ivars().reserved_width.get() > 0.0;
    let mut hit_checks = Vec::new();
    for b in [&parent, &bubble, &overflow] {
        let p = b.convertPoint_toView(
            NSPoint::new(
                if std::ptr::eq(&**b, &*parent) {
                    5.0
                } else {
                    9.0
                },
                9.0,
            ),
            None,
        );
        let hit = root.hitTest(root.convertPoint_fromView(p, None));
        hit_checks.push(
            hit.as_deref()
                .is_some_and(|hit| std::ptr::eq(hit, &**b as &NSView)),
        );
    }
    let point = bubble.convertPoint_toView(NSPoint::new(9.0, 9.0), None);
    let close_target =
        hover_close::tile_at(&state.borrow().bars[0], panel.convertPointToScreen(point)) == Some(2);
    bubble.mouseDown(&mouse_event(&panel, NSEventType::LeftMouseDown, point));
    bubble.mouseUp(&mouse_event(&panel, NSEventType::LeftMouseUp, point));
    let instant = state.borrow().focused_id() == Some(2)
        && parent.ivars().window_style.get().is_some_and(|v| v.focused)
        && bubble.ivars().window_style.get().is_some_and(|v| v.focused);
    for _ in 0..100 {
        autoreleasepool(|_| render(state));
    }
    let reused = state.borrow().performance.buttons_created == created;
    let screenshot = render_fixture_view(&root, "related-bubbles.png");
    {
        let mut s = state.borrow_mut();
        s.config.scale = 60.0;
        s.config.max_width = 60.0;
        s.config.show_titles = false;
    }
    render(state);
    let narrow = {
        let s = state.borrow();
        let b = &s.bars[0];
        !b.buttons.keys().any(|a| matches!(a, Action::Bubble(_)))
            && b.buttons[&Action::RelatedMore(1)].title().to_string() == "+8"
            && (b.panel.frame().size.height - 19.2).abs() < 1.0
    };
    {
        let mut s = state.borrow_mut();
        s.config.scale = 100.0;
        s.config.max_width = 200.0;
        s.config.show_titles = true;
        s.snapshot.windows.retain(|w| w.id == 1 || w.id == 2);
        let main = &mut s.snapshot.windows[0];
        main.title = "Terminal".into();
        main.tab_count = 2;
        main.native_tabs = true;
        main.tabs = vec![
            crate::models::WindowTab {
                id: 101,
                resolved: true,
                title: "Build".into(),
                selected: true,
            },
            crate::models::WindowTab {
                id: 102,
                resolved: true,
                title: "Logs".into(),
                selected: false,
            },
        ];
        s.pending_focus = None;
    }
    render(state);
    let (tab, panel, root) = {
        let s = state.borrow();
        let b = &s.bars[0];
        (
            b.buttons[&Action::Tab(1, 102)].clone(),
            b.panel.clone(),
            b.root.clone(),
        )
    };
    let point = tab.convertPoint_toView(NSPoint::new(9.0, 9.0), None);
    let tab_hit = root
        .hitTest(root.convertPoint_fromView(point, None))
        .is_some_and(|hit| std::ptr::eq(&*hit, &*tab as &NSView));
    panel.orderFrontRegardless();
    let tab_close = hover_close::action_at(
        &state.borrow(),
        panel.convertPointToScreen(point),
        panel.windowNumber(),
    ) == Some(Action::CloseTab(1, 102));
    panel.orderOut(None);
    tab.mouseDown(&mouse_event(&panel, NSEventType::LeftMouseDown, point));
    tab.mouseUp(&mouse_event(&panel, NSEventType::LeftMouseUp, point));
    let tab_feedback = tab.ivars().window_style.get().is_some_and(|s| s.focused)
        && state
            .borrow()
            .pending_tab
            .is_some_and(|(_, id, _)| id == 102);
    render(state);
    let pending_stable = state.borrow().bars[0].buttons[&Action::Tab(1, 102)]
        .ivars()
        .window_style
        .get()
        .is_some_and(|s| s.focused);
    let native_screenshot = render_fixture_view(&root, "related-bubbles.png");
    {
        let mut s = state.borrow_mut();
        (s.config, s.snapshot, s.order, s.pending_focus) = saved;
        s.pending_tab = None;
    }
    render(state);
    serde_json::json!({"passed":compact&&hit_checks.iter().all(|v|*v)&&close_target&&instant&&reused&&narrow&&screenshot&&tab_hit&&tab_close&&tab_feedback&&pending_stable&&native_screenshot,"compact":compact,"hit_checks":hit_checks,"hover_close_specific":close_target,"instant_feedback":instant,"controls_reused":reused,"narrow_overflow":narrow,"native_tab_hit":tab_hit,"native_tab_close":tab_close,"native_tab_feedback":tab_feedback,"pending_feedback_stable":pending_stable,"synthetic_screenshot":native_screenshot})
}

fn check_system_features(state: &Shared) -> serde_json::Value {
    let saved = state.borrow().config.clone();
    build_preferences(state, 3);
    let capture_control = state
        .borrow()
        .preferences
        .as_ref()
        .unwrap()
        .body
        .subviews()
        .iter()
        .any(|view| {
            view.downcast_ref::<ActionButton>()
                .is_some_and(|button| button.ivars().action == Action::Cycle("capture_mode".into()))
        });
    let capture_description_clear = {
        let s = state.borrow();
        let views = s.preferences.as_ref().unwrap().body.subviews();
        let description = views.iter().find(|view| {
            view.downcast_ref::<NSTextField>().is_some_and(|label| {
                label
                    .stringValue()
                    .to_string()
                    .starts_with("Stream reuses live frames.")
            })
        });
        let permissions: Vec<_> = views
            .iter()
            .filter(|view| {
                view.downcast_ref::<ActionButton>().is_some_and(|button| {
                    matches!(
                        button.ivars().action,
                        Action::Accessibility | Action::ScreenPermission
                    )
                })
            })
            .collect();
        permissions.len() == 2
            && description.is_some_and(|description| {
                permissions.iter().all(|button| {
                    !objc2_foundation::NSIntersectsRect(description.frame(), button.frame())
                })
            })
    };
    let previous = state.borrow().config.capture_mode;
    dispatch(state, Action::Cycle("capture_mode".into()));
    let capture_changes = state.borrow().config.capture_mode != previous;
    build_preferences(state, 2);
    let shortcuts_control = state
        .borrow()
        .preferences
        .as_ref()
        .unwrap()
        .body
        .subviews()
        .iter()
        .any(|view| {
            view.downcast_ref::<ActionButton>()
                .is_some_and(|button| button.ivars().action == Action::OpenShortcuts)
        });
    let shortcut_install_controls = [
        crate::system_actions::SystemAction::Sort,
        crate::system_actions::SystemAction::Show,
        crate::system_actions::SystemAction::Hide,
    ]
    .iter()
    .all(|action| {
        state
            .borrow()
            .preferences
            .as_ref()
            .unwrap()
            .body
            .subviews()
            .iter()
            .any(|view| {
                view.downcast_ref::<ActionButton>().is_some_and(|button| {
                    button.ivars().action == Action::InstallShortcut(*action)
                        && button.frame().origin.y >= 0.0
                })
            })
    });
    state.borrow_mut().received_snapshot = false;
    system_action(state, crate::system_actions::SystemAction::Sort);
    let sort_defers = state.borrow().pending_system_sort && !state.borrow().received_snapshot;
    state.borrow_mut().config.show_menubar = false;
    system_action(state, crate::system_actions::SystemAction::Hide);
    let hide = state.borrow().config.hidden_displays.len() == screens().len()
        && state.borrow().config.show_menubar;
    system_action(state, crate::system_actions::SystemAction::Show);
    let show = state.borrow().config.hidden_displays.is_empty();
    {
        let mut s = state.borrow_mut();
        s.config = saved;
        s.pending_system_sort = false;
        s.received_snapshot = true;
        if let Some(preferences) = s.preferences.take() {
            preferences.panel.close();
        }
    }
    serde_json::json!({"passed":capture_control && capture_description_clear && capture_changes && shortcuts_control && shortcut_install_controls && sort_defers && hide && show,
        "capture_control":capture_control,"capture_changes":capture_changes,"shortcuts_control":shortcuts_control,
        "capture_description_clear":capture_description_clear,
        "shortcut_install_controls":shortcut_install_controls,
        "sort_defers_until_snapshot":sort_defers,"hide_all":hide,"show_all":show})
}

fn check_start_search(state: &Shared) -> serde_json::Value {
    let saved = {
        let mut s = state.borrow_mut();
        let saved = (s.config.clone(), s.apps.clone());
        s.apps = ["Éditor", "Terminal", "東京"]
            .iter()
            .enumerate()
            .map(|(i, name)| Application {
                name: (*name).into(),
                bundle: format!("qa.search.{i}"),
                path: String::new(),
            })
            .collect();
        s.config.pins = vec![crate::config::Pin {
            bundle: "qa.search.1".into(),
            action: "launchOrActivateApp".into(),
        }];
        s.config.recent = vec!["qa.search.0".into()];
        s.config.app_order = vec!["qa.search.1".into()];
        s.last_catalog = Instant::now();
        saved
    };
    let display = state.borrow().bars[0].display;
    build_start(state, display);
    let mut checks = Vec::new();
    let mut unchanged_reuses = 0;
    for (purpose, query, expected) in [
        (
            PickerPurpose::Launch,
            "",
            vec![
                "qa.search.1",
                "qa.search.0",
                "qa.search.0",
                "qa.search.1",
                "qa.search.2",
            ],
        ),
        (PickerPurpose::Launch, "ÉD", vec!["qa.search.0"]),
        (PickerPurpose::Launch, "東京", vec!["qa.search.2"]),
        (PickerPurpose::Launch, "missing", vec![]),
        (
            PickerPurpose::Blacklist,
            "",
            vec!["qa.search.0", "qa.search.1", "qa.search.2"],
        ),
        (
            PickerPurpose::AppOrder,
            "",
            vec!["qa.search.0", "qa.search.2"],
        ),
    ] {
        {
            let mut s = state.borrow_mut();
            let menu = s.start.as_mut().unwrap();
            menu.purpose = purpose;
            menu.query = "\0".into();
            menu.search.setStringValue(&NSString::from_str(query));
        }
        search_start(state);
        let s = state.borrow();
        let menu = s.start.as_ref().unwrap();
        let actual: Vec<_> = menu
            .choices
            .iter()
            .filter_map(|action| match action {
                Action::Pin(bundle, _)
                | Action::Blacklist(bundle)
                | Action::AppendOrderApp(bundle) => Some(bundle.as_str()),
                _ => None,
            })
            .collect();
        checks.push(actual == expected && menu.selected == 0 && menu.query == query);
        let before = menu.body.subviews();
        drop(s);
        search_start(state);
        let after = state.borrow().start.as_ref().unwrap().body.subviews();
        if before.len() == after.len()
            && before
                .iter()
                .zip(after)
                .all(|(a, b)| std::ptr::eq(&*a, &*b))
        {
            unchanged_reuses += 1;
        }
    }
    let mut s = state.borrow_mut();
    if let Some(menu) = s.start.take() {
        menu.panel.close();
    }
    s.config = saved.0;
    s.apps = saved.1;
    serde_json::json!({"passed":checks.iter().all(|passed| *passed)&&unchanged_reuses==checks.len(),"cases":checks.len(),"unchanged_native_reuses":unchanged_reuses})
}

fn check_snapshot_updates(state: &Shared, count: usize) -> serde_json::Value {
    // This final workload uses only synthetic records and the hidden preview.
    let mut s = state.borrow_mut();
    s.config.thumbnails = true;
    s.config.reset_space_order = true;
    s.pending_focus = None;
    s.hover = None;
    s.preview_id = None;
    s.preview_cache.clear();
    let mut cases = Vec::new();
    for windows in [8, 32, 64, 512] {
        s.order.clear();
        s.known_spaces.clear();
        s.activity = crate::window_order::Activity::default();
        let snapshot = Snapshot {
            trusted: true,
            screen_allowed: true,
            windows: (1..=windows)
                .map(|id| Window {
                    id,
                    pid: 42,
                    title: format!("Snapshot QA {id}"),
                    app: "Snapshot QA".into(),
                    bundle: "com.apple.TextEdit".into(),
                    focused: id == 1,
                    on_space: true,
                    ..Window::default()
                })
                .collect(),
            ..Snapshot::default()
        };
        s.snapshot = Snapshot::default();
        apply_snapshot(&mut s, snapshot.clone());
        let order = s.order.clone();
        let mut timing = Timing::default();
        let mut correct = true;
        for cycle in 0..count {
            let mut next = snapshot.clone();
            next.scan_ms = cycle as u128;
            s.dirty = false;
            let start = Instant::now();
            let permission_change = apply_snapshot(&mut s, next);
            timing.record(start.elapsed());
            correct &= !permission_change
                && !s.dirty
                && s.order == order
                && s.known_spaces.len() == windows as usize
                && s.activity.len() == 1
                && s.snapshot.scan_ms == cycle as u128;
        }
        let mut changed_timing = Timing::default();
        for cycle in 0..count {
            let mut next = snapshot.clone();
            for window in &mut next.windows {
                window.on_space = cycle % 2 == 1;
            }
            s.dirty = false;
            let start = Instant::now();
            let permission_change = apply_snapshot(&mut s, next);
            changed_timing.record(start.elapsed());
            correct &= !permission_change && s.dirty && s.order == order;
        }
        cases.push(serde_json::json!({"windows":windows,"passed":correct,"unchanged_snapshot":timing.json(),"changed_snapshot":changed_timing.json()}));
    }
    let mut next = s.snapshot.clone();
    next.windows.truncate(3);
    apply_snapshot(&mut s, next);
    s.order = vec![3, 1, 2];
    s.hidden_now.insert(1);
    s.pending_focus = Some(active_feedback::PendingFocus::new(2, Instant::now()));
    let mut next = s.snapshot.clone();
    next.windows[0].title = "Changed title".into();
    next.windows[0].focused = false;
    next.windows[1].focused = true;
    next.windows[2].on_space = false;
    s.dirty = false;
    apply_snapshot(&mut s, next);
    let changed_windows = s.dirty
        && s.snapshot.windows[0].title == "Changed title"
        && s.order == [1, 2, 3]
        && !s.known_spaces[&3]
        && s.hidden_now.is_empty()
        && s.pending_focus.is_none()
        && s.activity.len() == 2;
    let mut next = s.snapshot.clone();
    next.windows.remove(0);
    next.windows.push(Window {
        id: 4,
        ..Window::default()
    });
    apply_snapshot(&mut s, next);
    let membership_changes = s.order == [2, 3, 4]
        && !s.known_spaces.contains_key(&1)
        && s.known_spaces.contains_key(&4)
        && s.activity.len() == 1;
    let mut next = s.snapshot.clone();
    next.badges.insert("com.apple.TextEdit".into(), "2".into());
    s.dirty = false;
    apply_snapshot(&mut s, next);
    let badge_change = s.dirty && s.snapshot.badges["com.apple.TextEdit"] == "2";
    let mut next = s.snapshot.clone();
    next.control_error = Some("Snapshot QA control failure".into());
    s.pending_focus = Some(active_feedback::PendingFocus::new(3, Instant::now()));
    s.dirty = false;
    apply_snapshot(&mut s, next);
    let control_failure =
        s.pending_focus.is_none() && s.dirty && s.error == "Snapshot QA control failure";
    let mut next = s.snapshot.clone();
    next.control_error = None;
    s.pending_focus = Some(active_feedback::PendingFocus::new(
        3,
        Instant::now() - Duration::from_secs(3),
    ));
    s.dirty = false;
    apply_snapshot(&mut s, next);
    let expired_feedback = s.pending_focus.is_none() && s.dirty;
    let Some(stock) = NSImage::imageNamed(&NSString::from_str("NSComputer")) else {
        return serde_json::json!({"passed":false,"error":"Missing snapshot fixture image"});
    };
    // SAFETY: the stock image stays alive throughout the query. Its retained
    // CGImage transfers ownership to Image, which releases it with CFRelease.
    let Some(bitmap) =
        (unsafe { stock.CGImageForProposedRect_context_hints(std::ptr::null_mut(), None, None) })
    else {
        return serde_json::json!({"passed":false,"error":"Missing snapshot fixture bitmap"});
    };
    let bytes = objc2_core_graphics::CGImage::bytes_per_row(Some(&bitmap))
        .saturating_mul(objc2_core_graphics::CGImage::height(Some(&bitmap)));
    let image = platform::Image(Retained::into_raw(bitmap).cast());
    let seed = |s: &mut State| {
        s.preview_cache
            .insert(2, image.clone(), bytes, Instant::now());
        s.hover = Some((2, 0, Instant::now()));
        s.preview_id = Some(2);
        s.preview.as_ref().unwrap().image.setImage(Some(&stock));
    };
    let released = |s: &State| {
        s.hover.is_none()
            && s.preview_id.is_none()
            && s.preview_cache.bytes() == 0
            && s.preview.as_ref().unwrap().image.image().is_none()
    };
    seed(&mut s);
    let mut next = s.snapshot.clone();
    next.screen_allowed = false;
    let permissions_changed = apply_snapshot(&mut s, next);
    let permission_cleanup = permissions_changed && released(&s);
    let mut next = s.snapshot.clone();
    next.screen_allowed = true;
    apply_snapshot(&mut s, next);
    seed(&mut s);
    s.config.thumbnails = false;
    let next = s.snapshot.clone();
    apply_snapshot(&mut s, next);
    let disabled_cleanup = released(&s);
    s.config.thumbnails = true;
    seed(&mut s);
    let mut next = s.snapshot.clone();
    next.windows.retain(|w| w.id != 2);
    apply_snapshot(&mut s, next);
    let closed_cleanup = released(&s) && s.order == [3, 4] && s.activity.len() == 0;
    let passed = cases.iter().all(|case| case["passed"] == true)
        && changed_windows
        && membership_changes
        && badge_change
        && control_failure
        && expired_feedback
        && permission_cleanup
        && disabled_cleanup
        && closed_cleanup;
    serde_json::json!({"passed":passed,"cases":cases,"changed_windows":changed_windows,"membership_changes":membership_changes,"badge_change":badge_change,"control_failure":control_failure,"expired_feedback":expired_feedback,"permission_cleanup":permission_cleanup,"disabled_cleanup":disabled_cleanup,"closed_cleanup":closed_cleanup})
}

fn check_presentation_updates(state: &Shared) -> serde_json::Value {
    let saved = {
        let mut s = state.borrow_mut();
        let saved = (s.config.clone(), s.snapshot.clone(), s.pending_focus);
        s.pending_focus = None;
        s.config.indicate_minimized = true;
        s.config.indicate_hidden = true;
        s.config.show_hidden = true;
        s.config.show_all_spaces = true;
        saved
    };
    let mut cases = Vec::new();
    for (i, (title, minimized, hidden, spaces, show_titles, expected)) in [
        (
            "Cafe\u{301} 👩🏽‍💻 東京",
            false,
            false,
            false,
            true,
            "Cafe\u{301} 👩🏽‍💻 東京",
        ),
        ("Renamed — α", true, true, true, true, "⌑ [Renamed — α]"),
        ("", false, true, false, true, "(QA — App)"),
        ("Full tooltip stays available", true, false, true, false, ""),
        ("Restored", false, false, false, true, "Restored"),
    ]
    .into_iter()
    .enumerate()
    {
        let theme = ["dark", "light", "auto"][i % 3];
        let bundle = if i == 4 {
            "com.apple.Safari"
        } else {
            "com.apple.TextEdit"
        };
        {
            let mut s = state.borrow_mut();
            s.config.show_titles = show_titles;
            s.config.theme = theme.into();
            s.config.font_size = 13.0 + i as f64 * 0.25;
            if i % 2 == 0 {
                s.snapshot.badges.insert(bundle.into(), "1".into());
            } else {
                s.snapshot.badges.clear();
            }
            s.snapshot.windows = vec![Window {
                id: 1,
                bundle: bundle.into(),
                app: "QA — App".into(),
                title: title.into(),
                minimized,
                hidden,
                all_spaces: spaces,
                on_space: true,
                ..Window::default()
            }];
        }
        render(state);
        let (button, tooltip, icon, correct) = {
            let s = state.borrow();
            let bar = &s.bars[0];
            let button = &bar.buttons[&Action::Window(1)];
            let tooltip = button.toolTip().unwrap();
            let appearance = bar.panel.appearance();
            // SAFETY: AppKit's immutable appearance-name constants.
            let name = unsafe {
                match theme {
                    "dark" => Some(NSAppearanceNameDarkAqua),
                    "light" => Some(NSAppearanceNameAqua),
                    _ => None,
                }
            };
            let theme_correct = match (appearance.as_deref(), name) {
                (Some(appearance), Some(name)) => appearance.name().isEqualToString(name),
                (None, None) => true,
                _ => false,
            };
            let correct = button.title().to_string() == expected
                && tooltip.to_string()
                    == format!("QA — App — {title}\n⌘W while hovering closes this window")
                && button.imagePosition()
                    == if show_titles {
                        NSCellImagePosition::ImageLeft
                    } else {
                        NSCellImagePosition::ImageOnly
                    }
                && button.imageScaling() == NSImageScaling::ScaleProportionallyUpOrDown
                && button
                    .image()
                    .is_some_and(|image| std::ptr::eq(&*image, &*s.icons[bundle]))
                && button.alphaValue() == if minimized || hidden { 0.65 } else { 1.0 }
                && button
                    .font()
                    .is_some_and(|font| font.pointSize() == s.config.font_size)
                && button.ivars().badge.get() == (i % 2 == 0)
                && theme_correct;
            (button.clone(), tooltip, button.image().unwrap(), correct)
        };
        render(state);
        let reused = {
            let s = state.borrow();
            let repeated = &s.bars[0].buttons[&Action::Window(1)];
            std::ptr::eq(&**repeated, &*button)
                && repeated
                    .toolTip()
                    .is_some_and(|current| std::ptr::eq(&*current, &*tooltip))
                && repeated
                    .image()
                    .is_some_and(|current| std::ptr::eq(&*current, &*icon))
        };
        cases.push(serde_json::json!({"case":i,"passed":correct && reused,"metadata_updated":correct,"unchanged_native_values_reused":reused}));
    }
    {
        let mut s = state.borrow_mut();
        (s.config, s.snapshot, s.pending_focus) = saved;
    }
    render(state);
    serde_json::json!({"passed":cases.iter().all(|case| case["passed"] == true),"cases":cases})
}

fn check_sort(state: &Shared) -> serde_json::Value {
    {
        let mut s = state.borrow_mut();
        s.config.show_sort = true;
        s.config.compact_related_windows = false;
        s.config.group_by_app = true;
        s.config.pins = vec![Pin {
            bundle: "com.apple.finder".into(),
            action: "launchOrActivateApp".into(),
        }];
        s.config.app_order = vec!["com.apple.Safari".into(), "com.apple.TextEdit".into()];
        s.config.app_order_initialized = true;
        s.snapshot.windows = (1..=5)
            .map(|id| Window {
                id,
                pid: if id < 4 { 1 } else { 2 },
                bundle: if id < 4 {
                    "com.apple.TextEdit"
                } else {
                    "com.apple.Safari"
                }
                .into(),
                app: if id < 4 { "TextEdit" } else { "Safari" }.into(),
                title: format!("Sort fixture {id}"),
                subordinate: id == 3 || id == 5,
                minimized: id == 1,
                on_space: true,
                width: 300.0,
                height: 200.0,
                ..Window::default()
            })
            .collect();
        s.activity = crate::window_order::Activity::default();
        for id in [2, 4, 3] {
            for w in &mut s.snapshot.windows {
                w.focused = w.id == id;
            }
            let State {
                activity, snapshot, ..
            } = &mut *s;
            activity.observe(&snapshot.windows);
        }
        s.pending_focus = None;
        s.order = vec![3, 4, 1, 5, 2];
    }
    render(state);
    let (before, commands, created, button, root, panel) = {
        let s = state.borrow();
        let bar = &s.bars[0];
        (
            s.snapshot.windows.clone(),
            s.performance.commands.get(),
            s.performance.buttons_created,
            bar.buttons[&Action::Sort].clone(),
            bar.root.clone(),
            bar.panel.clone(),
        )
    };
    let point = button.convertPoint_toView(
        NSPoint::new(
            button.bounds().size.width / 2.0,
            button.bounds().size.height / 2.0,
        ),
        None,
    );
    let discoverable = button.isBordered()
        && button.image().is_some()
        && button.usesSingleLineMode()
        && button.title().is_empty()
        && button.imagePosition() == NSCellImagePosition::ImageOnly
        && button.frame().size.width == button.frame().size.height + 4.0;
    let before_pins = {
        let s = state.borrow();
        let bar = &s.bars[0];
        let start = &bar.buttons[&Action::Start(bar.display)];
        let pin =
            &bar.buttons[&Action::Pin("com.apple.finder".into(), "launchOrActivateApp".into())];
        button.frame().origin.x == start.frame().origin.x + start.frame().size.width
            && pin.frame().origin.x == button.frame().origin.x + button.frame().size.width
    };
    let hit = root.hitTest(root.convertPoint_fromView(point, None));
    let hits = hit
        .as_deref()
        .is_some_and(|v| std::ptr::eq(v, &*button as &NSView));
    button.mouseDown(&mouse_event(&panel, NSEventType::LeftMouseDown, point));
    button.mouseUp(&mouse_event(&panel, NSEventType::LeftMouseUp, point));
    let immediate = state.borrow().order == [4, 5, 2, 1, 3];
    let mut displayed: Vec<_> = state.borrow().bars[0]
        .buttons
        .values()
        .filter_map(|b| match b.ivars().action {
            Action::Window(id) => Some((b.frame().origin.x, id)),
            _ => None,
        })
        .collect();
    displayed.sort_by(|a, b| a.0.total_cmp(&b.0));
    let native_order = displayed.iter().map(|(_, id)| *id).collect::<Vec<_>>() == [4, 5, 2, 1, 3];
    for _ in 0..200 {
        autoreleasepool(|_| dispatch(state, Action::Sort));
    }
    let (stable, state_unchanged, reused, history, timing) = {
        let s = state.borrow();
        (
            s.order == [4, 5, 2, 1, 3],
            s.snapshot.windows == before && s.performance.commands.get() == commands,
            s.performance.buttons_created == created,
            s.activity.len(),
            s.performance.sort.json(),
        )
    };
    dispatch(state, Action::MoveOrderApp("com.apple.Safari".into(), true));
    let edited = state.borrow().config.app_order == ["com.apple.TextEdit", "com.apple.Safari"];
    dispatch(state, Action::AppendOrderApp("com.apple.finder".into()));
    dispatch(state, Action::AppendOrderApp("com.apple.finder".into()));
    let unique = state
        .borrow()
        .config
        .app_order
        .iter()
        .filter(|b| *b == "com.apple.finder")
        .count()
        == 1;
    dispatch(state, Action::RemoveOrderApp("com.apple.finder".into()));
    let removed = !state
        .borrow()
        .config
        .app_order
        .contains(&"com.apple.finder".into());
    dispatch(state, Action::UseTaskOrder);
    let copied = state.borrow().config.app_order == ["com.apple.Safari", "com.apple.TextEdit"];
    let editor_rendered = {
        let s = state.borrow();
        let p = s.preferences.as_ref().unwrap();
        let in_bounds = p.body.subviews().iter().all(|v| v.frame().origin.y >= 0.0);
        in_bounds && render_fixture_view(&p.panel.contentView().unwrap(), "sort-order-editor.png")
    };
    // Stress the full bounded order list; all rows remain reachable in the scroll view.
    state.borrow_mut().config.app_order = (0..crate::window_order::MAX_APPS)
        .map(|i| format!("qa.app.{i}"))
        .collect();
    build_preferences(state, 10);
    let bounded_editor = {
        let s = state.borrow();
        let p = s.preferences.as_ref().unwrap();
        p.body.subviews().iter().all(|v| {
            v.frame().origin.y >= 0.0
                && v.frame().origin.y + v.frame().size.height <= p.body.frame().size.height
        })
    };
    // Exercise release/rebuild of complete settings windows, including all
    // 128 order rows, without touching the user's preferences or windows.
    let mut settings_released = true;
    for page in [10, 0, 3, 10].into_iter().cycle().take(20) {
        autoreleasepool(|_| {
            release_closed_preferences(&mut state.borrow_mut());
            settings_released &= state.borrow().preferences.is_none();
            build_preferences(state, page);
        });
    }
    release_closed_preferences(&mut state.borrow_mut());
    settings_released &= state.borrow().preferences.is_none();
    let passed = hits
        && discoverable
        && before_pins
        && immediate
        && native_order
        && stable
        && state_unchanged
        && reused
        && history == 3
        && edited
        && unique
        && removed
        && copied
        && editor_rendered
        && bounded_editor
        && settings_released;
    serde_json::json!({"passed":passed,"button_hit":hits,"compact_icon_button":discoverable,"after_start_before_pins":before_pins,"synchronous_sort":immediate,"native_button_order":native_order,"repeated_sort_stable":stable,"no_window_commands_or_state_changes":state_unchanged,"buttons_reused":reused,"activity_entries":history,"move_edit":edited,"add_unique":unique,"remove_edit":removed,"copy_task_order":copied,"editor_rendered":editor_rendered,"max_order_editor_in_bounds":bounded_editor,"settings_release_cycles":20,"settings_released":settings_released,"timing":timing})
}

fn check_pins(state: &Shared) -> serde_json::Value {
    let bundles = ["com.apple.finder", "com.apple.TextEdit", "com.apple.Safari"];
    let configured: Vec<_> = bundles
        .iter()
        .map(|bundle| Pin {
            bundle: (*bundle).into(),
            action: "launchOrActivateApp".into(),
        })
        .collect();
    {
        let mut s = state.borrow_mut();
        s.config.pins = configured.clone();
        s.config.all_displays = false;
        s.config.show_hidden = false;
        s.config.blacklist = vec![bundles[1].into()];
        s.snapshot.windows.clear();
        s.pending_focus = None;
    }
    let mut cases = Vec::new();
    for (phase, windows, expected) in [
        ("no_windows", vec![], vec![0, 1, 2]),
        (
            "open_app",
            vec![Window {
                id: 1,
                bundle: bundles[1].into(),
                on_space: true,
                ..Window::default()
            }],
            vec![0, 2],
        ),
        (
            "minimized_hidden_off_space_and_display",
            vec![Window {
                id: 2,
                bundle: bundles[1].into(),
                minimized: true,
                hidden: true,
                x: -100_000.0,
                ..Window::default()
            }],
            vec![0, 2],
        ),
        (
            "two_apps_with_windows",
            vec![
                Window {
                    id: 2,
                    bundle: bundles[1].into(),
                    ..Window::default()
                },
                Window {
                    id: 3,
                    bundle: bundles[2].into(),
                    ..Window::default()
                },
            ],
            vec![0],
        ),
        (
            "last_window_closed",
            vec![Window {
                id: 3,
                bundle: bundles[2].into(),
                ..Window::default()
            }],
            vec![0, 1],
        ),
        ("all_windows_closed", vec![], vec![0, 1, 2]),
    ] {
        state.borrow_mut().snapshot.windows = windows;
        render(state);
        let s = state.borrow();
        let bar = &s.bars[0];
        let iw = bar.panel.frame().size.height + 4.0;
        let mut shown: Vec<_> = bar
            .buttons
            .values()
            .filter_map(|button| {
                if let Action::Pin(bundle, action) = &button.ivars().action {
                    Some((button.frame().origin.x, bundle.as_str(), action.as_str()))
                } else {
                    None
                }
            })
            .collect();
        shown.sort_by(|a, b| a.0.total_cmp(&b.0));
        let correct = shown.len() == expected.len()
            && shown
                .iter()
                .zip(&expected)
                .enumerate()
                .all(|(i, (pin, index))| {
                    pin.1 == bundles[*index]
                        && pin.2 == "launchOrActivateApp"
                        && pin.0 == (i + 2) as f64 * iw
                });
        let no_gap = bar.scroll.frame().origin.x == (expected.len() + 2) as f64 * iw;
        let released = bar.root.subviews().len() == expected.len() + 3;
        cases.push(serde_json::json!({"phase":phase,"passed":correct && no_gap && released,"visible_pins":shown.len(),"no_gap":no_gap,"hidden_views_released":released}));
    }
    let preserved = state.borrow().config.pins == configured;
    let passed = preserved && cases.iter().all(|case| case["passed"] == true);
    serde_json::json!({"passed":passed,"configuration_preserved":preserved,"cases":cases})
}

fn mouse_event(panel: &NSPanel, kind: NSEventType, point: NSPoint) -> Retained<NSEvent> {
    NSEvent::mouseEventWithType_location_modifierFlags_timestamp_windowNumber_context_eventNumber_clickCount_pressure(
        kind, point, NSEventModifierFlags::empty(), 0.0, panel.windowNumber(), None, 0, 1, 1.0,
    ).unwrap()
}

fn arrow_cursor() -> bool {
    std::ptr::eq(&*NSCursor::currentCursor(), &*NSCursor::arrowCursor())
}

fn check_focus_clicks(state: &Shared) -> serde_json::Value {
    let (windows, pending, hides) = {
        let mut s = state.borrow_mut();
        let saved = (
            s.snapshot.windows.clone(),
            s.pending_focus,
            s.config.click_hides_app,
        );
        for w in &mut s.snapshot.windows {
            w.focused = w.id == 1;
            w.hidden = false;
            w.minimized = false;
        }
        s.pending_focus = None;
        s.config.click_hides_app = false;
        saved
    };
    render(state);
    dispatch(state, Action::Window(2));
    dispatch(state, Action::Window(1));
    let stale_focus_reactivates = state.borrow().pending_focus.is_some_and(|p| p.id == 1);
    dispatch(state, Action::Window(1));
    let repeat_pending_reactivates = state.borrow().pending_focus.is_some_and(|p| p.id == 1);
    {
        let mut s = state.borrow_mut();
        s.pending_focus = None;
        for w in &mut s.snapshot.windows {
            w.focused = w.id == 64;
            w.minimized = w.id == 64;
        }
    }
    dispatch(state, Action::Window(64));
    let minimized_reactivates = state.borrow().pending_focus.is_some_and(|p| p.id == 64);
    {
        let mut s = state.borrow_mut();
        s.pending_focus = None;
        s.snapshot
            .windows
            .iter_mut()
            .find(|w| w.id == 64)
            .unwrap()
            .minimized = false;
    }
    dispatch(state, Action::Window(64));
    let confirmed_focus_toggles = state.borrow().pending_focus.is_none();
    state
        .borrow_mut()
        .snapshot
        .windows
        .iter_mut()
        .find(|w| w.id == 64)
        .unwrap()
        .hidden = true;
    dispatch(state, Action::Window(64));
    let hidden_reactivates = state.borrow().pending_focus.is_some_and(|p| p.id == 64);
    let font_size_changes = {
        let s = state.borrow();
        let b = &s.bars[0].buttons[&Action::Window(64)];
        b.set_window_focus(true, s.config.font_size + 1.0);
        b.font()
            .is_some_and(|font| font.pointSize() == s.config.font_size + 1.0)
    };
    {
        let mut s = state.borrow_mut();
        s.snapshot.windows = windows;
        s.pending_focus = pending;
        s.config.click_hides_app = hides;
    }
    render(state);
    serde_json::json!({"passed":stale_focus_reactivates && repeat_pending_reactivates && minimized_reactivates && confirmed_focus_toggles && hidden_reactivates && font_size_changes,"stale_focus_reactivates":stale_focus_reactivates,"repeat_pending_reactivates":repeat_pending_reactivates,"minimized_reactivates":minimized_reactivates,"confirmed_focus_toggles":confirmed_focus_toggles,"hidden_reactivates":hidden_reactivates,"font_size_changes":font_size_changes})
}

fn check_cursor_and_clicks(state: &Shared) -> serde_json::Value {
    let (panel, root, scroll, tasks, buttons) = {
        let mut s = state.borrow_mut();
        for window in &mut s.snapshot.windows {
            window.focused = false;
        }
        s.pending_focus = None;
        let bar = &s.bars[0];
        (
            bar.panel.clone(),
            bar.root.clone(),
            bar.scroll.clone(),
            bar.tasks.clone(),
            bar.buttons.clone(),
        )
    };
    let mut post_scroll_clicks = 0;
    let mut motion_clicks = 0;
    let mut cursor_recoveries = 0;
    let mut movement_recoveries = 0;
    let mut passed = !panel.styleMask().contains(NSWindowStyleMask::Resizable)
        && scroll
            .contentView()
            .documentCursor()
            .is_some_and(|cursor| std::ptr::eq(&*cursor, &*NSCursor::arrowCursor()));
    let movement = mouse_event(&panel, NSEventType::MouseMoved, NSPoint::new(5.0, 16.0));
    for view in [
        &*root as &NSView,
        &*scroll as &NSView,
        &*buttons[&Action::Window(64)] as &NSView,
    ] {
        NSCursor::openHandCursor().set();
        view.mouseMoved(&movement);
        let recovered = arrow_cursor();
        movement_recoveries += usize::from(recovered);
        passed &= recovered;
        NSCursor::IBeamCursor().set();
        view.cursorUpdate(&movement);
        let recovered = arrow_cursor();
        movement_recoveries += usize::from(recovered);
        passed &= recovered;
    }
    root.updateTrackingAreas();
    scroll.updateTrackingAreas();
    let root_area = root.trackingAreas().objectAtIndex(0);
    let scroll_area = scroll.trackingAreas().objectAtIndex(0);
    let root_size = root.frame().size;
    let scroll_size = scroll.frame().size;
    for cycle in 0..200 {
        root.setFrameSize(NSSize::new(
            root_size.width - (cycle % 2) as f64,
            root_size.height,
        ));
        scroll.setFrameSize(NSSize::new(
            scroll_size.width - (cycle % 2) as f64,
            scroll_size.height,
        ));
        root.updateTrackingAreas();
        scroll.updateTrackingAreas();
    }
    root.setFrameSize(root_size);
    scroll.setFrameSize(scroll_size);
    let tracking_reused = std::ptr::eq(&*root_area, &*root.trackingAreas().objectAtIndex(0))
        && std::ptr::eq(&*scroll_area, &*scroll.trackingAreas().objectAtIndex(0));
    let bounded_tracking = root.trackingAreas().len() == 1
        && scroll.trackingAreas().len() == 1
        && [&*root as &NSView, &*scroll as &NSView].iter().all(|view| {
            view.trackingAreas().objectAtIndex(0).options().contains(
                NSTrackingAreaOptions::MouseMoved
                    | NSTrackingAreaOptions::ActiveAlways
                    | NSTrackingAreaOptions::InVisibleRect,
            )
        });
    passed &= tracking_reused && bounded_tracking && panel.acceptsMouseMovedEvents();
    // A synthetic wheel event is delivered only to this app's hidden QA view;
    // it is never posted to the window server or another application.
    let wheel = objc2_core_graphics::CGEvent::new_scroll_wheel_event2(
        None,
        objc2_core_graphics::CGScrollEventUnit::Pixel,
        2,
        0,
        12,
        0,
    )
    .and_then(|event| NSEvent::eventWithCGEvent(&event));
    let scroll_recovered = wheel.is_some_and(|event| {
        NSCursor::openHandCursor().set();
        scroll.scrollWheel(&event);
        arrow_cursor()
    });
    passed &= scroll_recovered;
    for id in [1, 32, 64] {
        let button = &buttons[&Action::Window(id)];
        tasks.scrollRectToVisible(button.frame());
        let point =
            button.convertPoint_toView(NSPoint::new(button.bounds().size.width / 2.0, 16.0), None);
        let before = state.borrow().performance.commands.get();
        NSCursor::openHandCursor().set();
        let down = mouse_event(&panel, NSEventType::LeftMouseDown, point);
        let up = mouse_event(&panel, NSEventType::LeftMouseUp, point);
        let local = root.convertPoint_fromView(point, None);
        let hit = root.hitTest(local);
        // Resolve the native clip/scroll hit target after each scroll. AppKit
        // drops NSWindow.sendEvent input for hidden panels, so dispatch through
        // that target's real responder methods. QA intercepts window commands.
        if let Some(hit) = hit {
            passed &= std::ptr::eq(&*hit, &**button as &NSView);
            hit.mouseDown(&down);
            hit.mouseUp(&up);
        } else {
            passed = false;
        }
        let clicked = state.borrow().performance.commands.get() == before + 1
            && state.borrow().focused_id() == Some(id);
        let cursor = arrow_cursor();
        post_scroll_clicks += usize::from(clicked);
        cursor_recoveries += usize::from(cursor);
        passed &= clicked
            && cursor
            && button.acceptsFirstMouse(Some(&down))
            && !button.mouseDownCanMoveWindow();
    }
    let button = &buttons[&Action::Window(64)];
    let warming_visible_only = task_button_visible(button, &scroll)
        && !task_button_visible(&buttons[&Action::Window(1)], &scroll);
    passed &= warming_visible_only;
    for dragging in [false, true] {
        state.borrow_mut().config.task_dragging = dragging;
        let order = state.borrow().order.clone();
        let point =
            button.convertPoint_toView(NSPoint::new(button.bounds().size.width / 2.0, 16.0), None);
        let moved = NSPoint::new(point.x + 8.0, point.y);
        let before = state.borrow().performance.commands.get();
        button.mouseDown(&mouse_event(&panel, NSEventType::LeftMouseDown, point));
        button.mouseDragged(&mouse_event(&panel, NSEventType::LeftMouseDragged, moved));
        button.mouseUp(&mouse_event(&panel, NSEventType::LeftMouseUp, moved));
        let clicked = state.borrow().performance.commands.get() == before + 1
            && state.borrow().order == order
            && button.ivars().drag.get().is_none();
        motion_clicks += usize::from(clicked);
        passed &= clicked;
    }
    state.borrow_mut().config.task_dragging = true;
    let other = &buttons[&Action::Window(63)];
    let from =
        button.convertPoint_toView(NSPoint::new(button.bounds().size.width / 2.0, 16.0), None);
    let to = other.convertPoint_toView(NSPoint::new(other.bounds().size.width / 2.0, 16.0), None);
    let order = state.borrow().order.clone();
    let before = state.borrow().performance.commands.get();
    button.mouseDown(&mouse_event(&panel, NSEventType::LeftMouseDown, from));
    button.mouseDragged(&mouse_event(&panel, NSEventType::LeftMouseDragged, to));
    button.mouseUp(&mouse_event(&panel, NSEventType::LeftMouseUp, to));
    let reordered =
        state.borrow().order != order && state.borrow().performance.commands.get() == before;
    passed &= reordered;
    serde_json::json!({"passed":passed,"mode":"native_hit_test_and_handlers","post_scroll_clicks":post_scroll_clicks,"cursor_recoveries":cursor_recoveries,"movement_recoveries":movement_recoveries,"scroll_cursor_recovery":scroll_recovered,"bounded_tracking":bounded_tracking,"tracking_reused":tracking_reused,"motion_clicks":motion_clicks,"drag_to_other_button_reorders":reordered,"warming_visible_only":warming_visible_only})
}

fn check_minimized_preview(state: &Shared) -> serde_json::Value {
    let Some(stock) = NSImage::imageNamed(&NSString::from_str("NSComputer")) else {
        return serde_json::json!({"passed":false,"error":"Missing stock fixture image"});
    };
    // SAFETY: no proposed rectangle or hints are supplied. The retained stock
    // CGImage transfers its +1 ownership to Image, which releases it with CFRelease.
    let Some(bitmap) =
        (unsafe { stock.CGImageForProposedRect_context_hints(std::ptr::null_mut(), None, None) })
    else {
        return serde_json::json!({"passed":false,"error":"Missing stock fixture bitmap"});
    };
    let bytes = objc2_core_graphics::CGImage::bytes_per_row(Some(&bitmap))
        .saturating_mul(objc2_core_graphics::CGImage::height(Some(&bitmap)));
    let image = platform::Image(Retained::into_raw(bitmap).cast());
    let (number, anchor) = {
        let mut s = state.borrow_mut();
        s.config.thumbnails = true;
        s.config.hover_ms = 0;
        s.snapshot.screen_allowed = true;
        for w in &mut s.snapshot.windows {
            w.minimized = w.id == 63 || w.id == 64;
        }
        s.preview_cache
            .insert(64, image, bytes, Instant::now() - Duration::from_secs(3600));
        poll_images(&mut s);
        let bar = &s.bars[0];
        (
            bar.panel.windowNumber(),
            bar.panel.convertRectToScreen(rect(10.0, 0.0, 100.0, 32.0)),
        )
    };
    begin_hover(state, 64, number, anchor);
    let (cached_displayed, no_capture, retained_bytes) = {
        let s = state.borrow();
        (
            s.preview_id == Some(64)
                && s.preview
                    .as_ref()
                    .is_some_and(|p| p.image.image().is_some()),
            !s.capture_busy && crate::capture::counts().1 == 0,
            s.preview_cache.bytes(),
        )
    };
    let preview_metadata = {
        let mut s = state.borrow_mut();
        let saved = (
            s.config.thumbnail_font,
            s.config.thumbnail_scale,
            s.config.thumbnail_titles,
            s.snapshot
                .windows
                .iter()
                .find(|w| w.id == 64)
                .unwrap()
                .title
                .clone(),
        );
        let mut correct = true;
        for (font, scale, visible, title) in [
            (17.0, 130.0, true, "東京 👩🏽‍💻"),
            (13.0, 100.0, false, "Renamed preview"),
        ] {
            s.config.thumbnail_font = font;
            s.config.thumbnail_scale = scale;
            s.config.thumbnail_titles = visible;
            s.snapshot
                .windows
                .iter_mut()
                .find(|w| w.id == 64)
                .unwrap()
                .title = title.into();
            let image = s.preview_cache.get(64).unwrap().clone();
            show_preview_image(&mut s, 64, image);
            let p = s.preview.as_ref().unwrap();
            let native_title = p.title.stringValue();
            let native_font = p.title.font().unwrap();
            correct &= native_text_matches(&native_title, &[title])
                && native_font.pointSize() == font
                && p.title.isHidden() != visible
                && p.panel.frame().size.width == 280.0 * scale / 100.0;
            let image = s.preview_cache.get(64).unwrap().clone();
            show_preview_image(&mut s, 64, image);
            let p = s.preview.as_ref().unwrap();
            correct &= std::ptr::eq(&*native_title, &*p.title.stringValue())
                && std::ptr::eq(&*native_font, &*p.title.font().unwrap());
        }
        s.config.thumbnail_font = saved.0;
        s.config.thumbnail_scale = saved.1;
        s.config.thumbnail_titles = saved.2;
        s.snapshot
            .windows
            .iter_mut()
            .find(|w| w.id == 64)
            .unwrap()
            .title = saved.3;
        let image = s.preview_cache.get(64).unwrap().clone();
        show_preview_image(&mut s, 64, image);
        correct
    };
    let panel_number = state
        .borrow()
        .preview
        .as_ref()
        .unwrap()
        .panel
        .windowNumber();
    let gapless_position = state
        .borrow()
        .preview
        .as_ref()
        .unwrap()
        .panel
        .frame()
        .origin
        .y
        == anchor.origin.y + anchor.size.height;
    {
        let mut s = state.borrow_mut();
        let image = s.preview_cache.get(64).unwrap().clone();
        s.preview_cache.insert(63, image, bytes, Instant::now());
    }
    begin_hover(state, 63, number, anchor);
    let cached_switch = {
        let s = state.borrow();
        s.preview_id == Some(63)
            && !s.capture_busy
            && s.preview.as_ref().is_some_and(|p| {
                p.panel.isVisible()
                    && p.panel.windowNumber() == panel_number
                    && p.image.image().is_some()
            })
    };
    let cached_close_target = {
        let s = state.borrow();
        let frame = s.preview.as_ref().unwrap().panel.frame();
        hover_close::target_at(
            &s,
            NSPoint::new(frame.origin.x + 5.0, frame.origin.y + 5.0),
            panel_number,
        )
    };
    let cached_minimized_close = command_w_closes(cached_close_target, 63);
    hide_preview(state);
    state.borrow_mut().preview_cache.retain(|id| id != 63);
    begin_hover(state, 63, number, anchor);
    let cold_minimized_skips_capture = {
        let s = state.borrow();
        !s.capture_busy && s.preview_id.is_none() && crate::capture::counts().1 == 0
    };
    let cold_close_target = {
        let s = state.borrow();
        let bar = &s.bars[0];
        let button = &bar.buttons[&Action::Window(63)];
        let local = button.convertPoint_toView(
            NSPoint::new(
                button.bounds().size.width / 2.0,
                button.bounds().size.height / 2.0,
            ),
            None,
        );
        hover_close::tile_at(bar, bar.panel.convertPointToScreen(local))
    };
    let cold_minimized_close = command_w_closes(cold_close_target, 63);
    hide_preview(state);
    let (retained_after_restore, closed_released) = {
        let mut s = state.borrow_mut();
        s.snapshot
            .windows
            .iter_mut()
            .find(|w| w.id == 64)
            .unwrap()
            .minimized = false;
        poll_images(&mut s);
        let retained =
            s.preview_cache.get(64).is_some() && s.preview_cache.needs_refresh(64, Instant::now());
        s.snapshot.windows.retain(|w| w.id != 64);
        let alive = s.snapshot.windows.iter().map(|w| w.id).collect();
        prune_preview_sources(&mut s, &alive);
        (
            retained,
            s.preview_cache.get(64).is_none() && s.preview_cache.bytes() == 0,
        )
    };
    let passed = cached_displayed
        && preview_metadata
        && no_capture
        && gapless_position
        && cached_switch
        && cached_minimized_close
        && cold_minimized_close
        && cold_minimized_skips_capture
        && retained_after_restore
        && closed_released
        && retained_bytes <= preview_cache::MAX_BYTES;
    serde_json::json!({"passed":passed,"cached_image_age_seconds":3600,"cached_displayed":cached_displayed,"preview_metadata_updates_and_reuses":preview_metadata,"minimized_skips_capture":no_capture,"cold_minimized_skips_capture":cold_minimized_skips_capture,"cached_minimized_command_w":cached_minimized_close,"cold_minimized_command_w":cold_minimized_close,"retained_after_restore":retained_after_restore,"closed_releases_bitmap":closed_released,"cache_bytes":retained_bytes,"popup_meets_task_strip":gapless_position,"cached_switch_synchronous":cached_switch})
}

fn command_w_closes(target: Option<u32>, expected: u32) -> bool {
    use objc2_core_graphics::{CGEventFlags, CGEventType};
    let mut keys = hover_close::KeyState::default();
    keys.decision(
        CGEventType::KeyDown,
        13,
        CGEventFlags::MaskCommand,
        false,
        target,
    ) == hover_close::Decision::Close(expected)
        && keys.decision(CGEventType::KeyUp, 13, CGEventFlags::empty(), false, None)
            == hover_close::Decision::Consume
}

fn check_preview_frame(state: &Shared) -> serde_json::Value {
    let s = state.borrow();
    let preview = s.preview.as_ref().unwrap();
    let width = 280.0;
    let height = 150.0;
    let header = preview_frame::header_height(13.0);
    preview
        .panel
        .setContentSize(NSSize::new(width, height + header + 4.0));
    layout_preview(preview, width, height, 13.0);
    preview
        .title
        .setStringValue(&NSString::from_str("Taskbar QA — compact thumbnail frame"));
    let event = mouse_event(
        &preview.panel,
        NSEventType::LeftMouseDown,
        NSPoint::new(5.0, 5.0),
    );
    unsafe {
        let _: () = msg_send![&*preview.frame, mouseEntered: &*event];
    }
    let highlighted = preview.frame.hovered();
    let hovered_render = render_preview_fixture(preview, "preview-frame-hover.png");
    unsafe {
        let _: () = msg_send![&*preview.frame, mouseExited: &*event];
    }
    let cleared = !preview.frame.hovered();
    let normal_render = render_preview_fixture(preview, "preview-frame-normal.png");
    for _ in 0..200 {
        preview.frame.updateTrackingAreas();
    }
    let tracking = preview.frame.trackingAreas().len();
    let passed = highlighted
        && cleared
        && tracking == 1
        && hovered_render
        && normal_render
        && header == 20.0
        && preview.image.frame().origin == NSPoint::new(2.0, 2.0)
        && preview.title.usesSingleLineMode()
        && !preview.frame.mouseDownCanMoveWindow();
    serde_json::json!({"passed":passed,"header_height":header,"image_inset":preview.image.frame().origin.x,"highlight_enter":highlighted,"highlight_exit":cleared,"tracking_areas_after_200_updates":tracking})
}

fn render_preview_fixture(preview: &Preview, filename: &str) -> bool {
    if std::env::var_os("TASKBAR_QA_RENDER_DIR").is_none() {
        return true;
    }
    let image = NSImage::imageNamed(&NSString::from_str("NSComputer"));
    preview.image.setImage(image.as_deref());
    render_fixture_view(&preview.frame, filename)
}

fn render_fixture_view(view: &NSView, filename: &str) -> bool {
    let Some(directory) = std::env::var_os("TASKBAR_QA_RENDER_DIR") else {
        return true;
    };
    let directory = std::path::PathBuf::from(directory);
    let result = (|| -> Result<(), String> {
        std::fs::create_dir_all(&directory).map_err(|e| e.to_string())?;
        // Only our synthetic AppKit fixture view, never desktop pixels.
        let bounds = view.bounds();
        let bitmap = view
            .bitmapImageRepForCachingDisplayInRect(bounds)
            .ok_or("No native preview bitmap")?;
        view.cacheDisplayInRect_toBitmapImageRep(bounds, &bitmap);
        // SAFETY: the typed properties dictionary is empty; no untyped values cross the API.
        let png = unsafe {
            bitmap.representationUsingType_properties(
                NSBitmapImageFileType::PNG,
                &objc2_foundation::NSDictionary::new(),
            )
        }
        .ok_or("No native preview PNG")?;
        std::fs::write(directory.join(filename), png.to_vec()).map_err(|e| e.to_string())?;
        Ok(())
    })();
    result.is_ok()
}

pub(super) struct HoverBenchmark {
    pid: i32,
    total: usize,
    started: Instant,
    last: Instant,
    pending: Option<(u32, Instant)>,
    samples: Vec<serde_json::Value>,
    rapid_remaining: usize,
    transitions: usize,
    warming: bool,
    warmed_sources: usize,
    warmup_ms: Option<f64>,
    idle_since: Option<Instant>,
}
impl HoverBenchmark {
    pub(super) fn owns(&self, window: &Window) -> bool {
        window.pid == self.pid && window.bundle == "io.sharif.taskbarrust.interactionfixture"
    }
    pub(super) fn new(total: usize, pid: i32) -> Self {
        Self {
            pid,
            total,
            started: Instant::now(),
            last: Instant::now(),
            pending: None,
            samples: Vec::new(),
            rapid_remaining: 12,
            transitions: 0,
            warming: true,
            warmed_sources: 0,
            warmup_ms: None,
            idle_since: None,
        }
    }
}
pub(super) fn hover_tick(state: &Shared) {
    let Some(mut test) = state.borrow_mut().hover_benchmark.take() else {
        return;
    };
    let mut error = None;
    let mut enter = None;
    {
        let s = state.borrow();
        if test.started.elapsed() > Duration::from_secs(90) {
            error = Some("Native hover workload timed out".to_string());
        } else if test.started.elapsed() > Duration::from_secs(3) && !s.snapshot.screen_allowed {
            error = Some("Native hover process has no Screen Recording permission".to_string());
        } else if test.samples.len() >= test.total {
            // The completed workload waits for its native capture buffers to release.
        } else if test.warming {
            let ids: Vec<_> = s
                .snapshot
                .windows
                .iter()
                .filter(|w| test.owns(w) && !w.minimized)
                .map(|w| w.id)
                .collect();
            if ids.len() >= 2 && ids.iter().all(|id| s.preview_cache.get(*id).is_some()) {
                test.warming = false;
                test.warmed_sources = ids.len();
                test.warmup_ms = Some(test.started.elapsed().as_secs_f64() * 1000.0);
                test.last = Instant::now() - Duration::from_millis(300);
            } else if test.started.elapsed() > Duration::from_secs(20) {
                error = Some(format!(
                    "Fixture background thumbnails did not warm: {}",
                    s.error
                ));
            }
        } else if let Some((id, at)) = test.pending {
            if test.rapid_remaining > 0 && (s.capture_busy || s.preview_id == Some(id)) {
                // Change source during capture, then verify settled thumbnails.
                // This exercises cooperative cancellation without OS input.
                test.rapid_remaining -= 1;
                test.transitions += 1;
                test.pending = None;
                test.last = Instant::now() - Duration::from_millis(300);
            } else if s.preview_id == Some(id)
                && s.preview
                    .as_ref()
                    .is_some_and(|p| p.panel.isVisible() && p.image.image().is_some())
            {
                test.samples.push(serde_json::json!({"hover_ms":at.elapsed().as_secs_f64()*1000.0,"captures":s.captures,"cache_entries":s.preview_cache.len()}));
                test.pending = None;
                test.last = Instant::now();
                test.transitions += 1;
            } else if at.elapsed() > Duration::from_secs(8) {
                error = Some(format!("Fixture thumbnail did not appear: {}", s.error));
            }
        } else if test.last.elapsed() > Duration::from_millis(200) {
            let mut ids: Vec<_> = s
                .snapshot
                .windows
                .iter()
                .filter(|w| {
                    w.pid == test.pid
                        && w.bundle == "io.sharif.taskbarrust.interactionfixture"
                        && w.title.starts_with("Taskbar QA ")
                        && !w.minimized
                })
                .map(|w| w.id)
                .collect();
            ids.sort_unstable();
            if ids.len() >= 2 {
                let id = ids[test.transitions % ids.len()];
                enter = s.bars.iter().find_map(|b| {
                    b.buttons
                        .get(&Action::Window(id))
                        .map(|button| (id, button.clone(), b.panel.windowNumber()))
                });
            }
        }
    }
    if test.pending.is_none() {
        hide_preview(state);
    }
    if let Some((id, button, number)) = enter {
        let event=unsafe{NSEvent::enterExitEventWithType_location_modifierFlags_timestamp_windowNumber_context_eventNumber_trackingNumber_userData(NSEventType::MouseEntered,NSPoint::new(1.0,1.0),NSEventModifierFlags::empty(),0.0,number,None,0,0,std::ptr::null_mut())}.unwrap();
        let at = Instant::now();
        let cached = state.borrow().preview_cache.get(id).is_some();
        unsafe {
            let _: () = msg_send![&*button,mouseEntered:&*event];
        }
        let s = state.borrow();
        if test.rapid_remaining == 0 && s.preview_id == Some(id) {
            test.samples.push(serde_json::json!({"hover_ms":at.elapsed().as_secs_f64()*1000.0,"cached_before_hover":cached,"synchronous":true,"captures":s.captures,"cache_entries":s.preview_cache.len()}));
            test.last = Instant::now();
            test.transitions += 1;
        } else {
            test.pending = Some((id, at));
        }
    }
    if error.is_none() && test.samples.len() >= test.total {
        hide_preview(state);
        let idle_since = *test.idle_since.get_or_insert_with(|| {
            state.borrow_mut().prefetch_after = Instant::now() + Duration::from_secs(20);
            Instant::now()
        });
        let (_, _, live, active) = crate::capture::counts();
        if state.borrow().capture_busy || live != 0 || active {
            if idle_since.elapsed() < Duration::from_secs(15) {
                state.borrow_mut().hover_benchmark = Some(test);
                return;
            }
            error = Some("Idle capture session did not release its native buffers".into());
        }
    }
    if error.is_some() || test.samples.len() >= test.total {
        let s = state.borrow();
        let success = error.is_none()
            && s.capture_errors == 0
            && s.preview_cache.len() <= preview_cache::MAX_ENTRIES
            && s.preview_cache.bytes() <= preview_cache::MAX_BYTES
            && test.samples.iter().all(|sample| {
                sample["synchronous"] == true && sample["cached_before_hover"] == true
            });
        println!(
            "{}",
            serde_json::json!({"completed":true,"passed":success,"mode":"app_local_mouseEntered_with_native_capture","capture_mode":s.config.capture_mode,"capture_features":crate::capture::diagnostics(),"configured_delay_ms":s.config.hover_ms,"rapid_source_changes":12-test.rapid_remaining,"warmed_sources":test.warmed_sources,"warmup_ms":test.warmup_ms,"samples":test.samples,"performance":s.performance.json(),"cache_bytes":s.preview_cache.bytes(),"error":error,"capture_errors":s.capture_errors,"stream_counts":crate::capture::counts(),"idle_buffers_released":error.is_none()&&test.idle_since.is_some()})
        );
        drop(s);
        if !success {
            state.borrow_mut().error =
                error.unwrap_or_else(|| "Native hover workload failed".into());
        }
        NSApplication::sharedApplication(MainThreadMarker::new().unwrap()).terminate(None);
    } else {
        state.borrow_mut().hover_benchmark = Some(test);
    }
}
