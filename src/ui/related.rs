//! Small, retained controls for related windows; capture stays on demand.
use super::*;

enum Entry<'a> {
    Window(&'a Window),
    Tab(usize, &'a crate::models::WindowTab),
    Remaining(usize),
}
fn entries<'a>(main: &'a Window, children: &[&'a Window]) -> Vec<Entry<'a>> {
    let mut result = Vec::new();
    if main.tab_count > 1 {
        result.extend(main.tabs.iter().enumerate().map(|(i, t)| Entry::Tab(i, t)));
        if main.tab_count > main.tabs.len() {
            result.push(Entry::Remaining(main.tab_count - main.tabs.len()));
        }
    }
    result.extend(
        children
            .iter()
            .filter(|w| !w.tabbed_hidden || !main.tabs.iter().any(|t| t.title == w.title))
            .map(|w| Entry::Window(w)),
    );
    result
}
fn abbreviation(title: &str) -> String {
    let text: String = title
        .split_whitespace()
        .filter_map(|word| word.chars().next())
        .take(2)
        .flat_map(char::to_uppercase)
        .take(2)
        .collect();
    if text.is_empty() { "•".into() } else { text }
}
fn selected(state: &State, owner: u32, index: u64, actual: bool) -> bool {
    state
        .pending_tab
        .filter(|(id, _, at)| *id == owner && at.elapsed() < Duration::from_secs(2))
        .map_or(actual, |(_, pending, _)| pending == index)
}

pub(super) fn render_bubbles(
    state: &Shared,
    group: &crate::related_windows::Group<'_>,
    frame: NSRect,
    focused: Option<u32>,
    old: &mut HashMap<Action, Retained<ActionButton>>,
    buttons: &mut HashMap<Action, Retained<ActionButton>>,
    children: &mut Vec<Retained<NSView>>,
) -> f64 {
    let entries = entries(group.main, &group.children);
    if entries.is_empty() {
        return 0.0;
    }
    let scale = frame.size.height / 32.0;
    let size = 18.0 * scale;
    let step = 21.0 * scale;
    let slots = ((frame.size.width - 30.0 * scale) / step)
        .floor()
        .clamp(1.0, 3.0) as usize;
    let overflow = entries.len() > slots;
    let shown = if overflow { slots - 1 } else { entries.len() };
    let count = shown + usize::from(overflow);
    let reserved = count as f64 * step + 3.0 * scale;
    for index in 0..count {
        let (action, title, tooltip, active) = if index < shown {
            match &entries[index] {
                Entry::Window(w) => (
                    Action::Bubble(w.id),
                    abbreviation(&w.title),
                    format!(
                        "{} — {}\nClick to show this window. ⌘W while hovering closes it.",
                        w.app, w.title
                    ),
                    focused == Some(w.id),
                ),
                Entry::Tab(i, t) => (
                    Action::Tab(group.main.id, t.id),
                    (i + 1).to_string(),
                    format!(
                        "{} — Tab {}: {}\nClick to select this tab. ⌘W while hovering closes it.",
                        group.main.app,
                        i + 1,
                        t.title
                    ),
                    selected(&state.borrow(), group.main.id, t.id, t.selected),
                ),
                Entry::Remaining(n) => (
                    Action::RelatedMore(group.main.id),
                    "…".into(),
                    format!("Open the application to see {n} more tabs"),
                    false,
                ),
            }
        } else {
            let remaining = entries.len() - shown;
            (
                Action::RelatedMore(group.main.id),
                if remaining < 10 {
                    format!("+{remaining}")
                } else {
                    "9+".into()
                },
                format!("Show {remaining} related windows and tabs"),
                false,
            )
        };
        let new = !old.contains_key(&action);
        let b = reuse_button(
            state,
            old,
            action.clone(),
            &title,
            rect(
                frame.origin.x + frame.size.width - reserved + index as f64 * step,
                frame.size.height - size - 2.0 * scale,
                size,
                size,
            ),
        );
        set_tooltip(&b, &[&tooltip]);
        // SAFETY: all buttons implement AppKit's accessibility label selector.
        unsafe {
            let _: () = msg_send![&*b,setAccessibilityLabel:&*NSString::from_str(&tooltip)];
        }
        b.set_window_focus(active, 9.0 * scale);
        if new {
            let tracking = unsafe {
                NSTrackingArea::initWithRect_options_owner_userInfo(
                    NSTrackingArea::alloc(),
                    b.bounds(),
                    NSTrackingAreaOptions::MouseEnteredAndExited
                        | NSTrackingAreaOptions::MouseMoved
                        | NSTrackingAreaOptions::ActiveAlways
                        | NSTrackingAreaOptions::InVisibleRect,
                    Some(&b),
                    None,
                )
            };
            b.addTrackingArea(&tracking);
        }
        children.push(as_view(&b));
        buttons.insert(action, b);
    }
    reserved
}

pub(super) fn select_tab(state: &Shared, id: u32, tab_id: u64) {
    let started = Instant::now();
    let buttons = {
        let mut s = state.borrow_mut();
        if !s
            .snapshot
            .windows
            .iter()
            .any(|w| w.id == id && w.tabs.iter().any(|t| t.id == tab_id))
            || !s.command(Command::SelectTab(tab_id))
        {
            return;
        }
        s.pending_tab = Some((id, tab_id, started));
        s.pending_focus = Some(active_feedback::PendingFocus::new(id, started));
        s.bars
            .iter()
            .flat_map(|b| b.buttons.values())
            .filter_map(|b| match b.ivars().action {
                Action::Tab(owner, i) if owner == id => {
                    Some((b.clone(), i == tab_id, 9.0 * s.config.scale / 100.0))
                }
                Action::Window(owner) if owner == id => Some((b.clone(), true, s.config.font_size)),
                _ => None,
            })
            .collect::<Vec<_>>()
    };
    for (b, active, size) in buttons {
        b.set_window_focus(active, size);
        b.displayIfNeeded();
    }
    state
        .borrow_mut()
        .performance
        .click_feedback
        .record(started.elapsed());
}

pub(super) fn show_more(state: &Shared, id: u32) {
    let (owner, items) = {
        let s = state.borrow();
        let owner = s
            .bars
            .iter()
            .find_map(|bar| bar.buttons.get(&Action::RelatedMore(id)))
            .cloned();
        let ids = s
            .bars
            .iter()
            .find_map(|bar| bar.buttons.get(&Action::Window(id)))
            .map(|b| b.ivars().related_ids.borrow().clone())
            .unwrap_or_default();
        let children = ids
            .iter()
            .filter_map(|id| s.snapshot.windows.iter().find(|w| w.id == *id))
            .collect::<Vec<_>>();
        let items = s
            .snapshot
            .windows
            .iter()
            .find(|w| w.id == id)
            .map(|w| {
                entries(w, &children)
                    .into_iter()
                    .map(|entry| match entry {
                        Entry::Window(w) => (
                            if w.title.is_empty() {
                                w.app.clone()
                            } else {
                                w.title.clone()
                            },
                            Action::Raise(w.id),
                        ),
                        Entry::Tab(i, t) => (
                            format!("Tab {} — {}", i + 1, t.title),
                            Action::Tab(id, t.id),
                        ),
                        Entry::Remaining(n) => (
                            format!("Open application for {n} more tabs…"),
                            Action::Raise(id),
                        ),
                    })
                    .collect::<Vec<_>>()
            })
            .unwrap_or_default();
        (owner, items)
    };
    let Some(owner) = owner else {
        return;
    };
    let menu = NSMenu::new(MainThreadMarker::new().unwrap());
    for (title, action) in items {
        menu_item(state, &menu, &title, action);
    }
    menu.popUpMenuPositioningItem_atLocation_inView(
        None,
        NSPoint::new(0.0, owner.bounds().size.height),
        Some(&owner),
    );
}
