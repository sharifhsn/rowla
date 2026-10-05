//! Saved application precedence and an explicit, immediate restore action.
use super::*;

pub(super) fn button_icon() -> Retained<NSImage> {
    // Resolution-independent template: descending rows plus an ordering arrow.
    // The block captures no state; the button owns/reuses this small image.
    let draw = block2::RcBlock::new(|_bounds: NSRect| {
        NSColor::blackColor().set();
        for (y, width) in [(13.0, 9.0), (8.0, 7.0), (3.0, 5.0)] {
            NSBezierPath::bezierPathWithRoundedRect_xRadius_yRadius(
                rect(1.0, y, width, 2.0),
                1.0,
                1.0,
            )
            .fill();
        }
        let arrow = NSBezierPath::bezierPath();
        arrow.setLineWidth(1.5);
        arrow.setLineCapStyle(NSLineCapStyle::Round);
        arrow.setLineJoinStyle(NSLineJoinStyle::Round);
        arrow.moveToPoint(NSPoint::new(14.0, 14.0));
        arrow.lineToPoint(NSPoint::new(14.0, 3.5));
        arrow.moveToPoint(NSPoint::new(11.5, 6.0));
        arrow.lineToPoint(NSPoint::new(14.0, 3.5));
        arrow.lineToPoint(NSPoint::new(16.5, 6.0));
        arrow.stroke();
        objc2::runtime::Bool::YES
    });
    let image =
        NSImage::imageWithSize_flipped_drawingHandler(NSSize::new(18.0, 18.0), false, &draw);
    image.setTemplate(true);
    image
}

pub(super) fn apply(state: &Shared) {
    let started = Instant::now();
    {
        let mut s = state.borrow_mut();
        let State {
            order,
            snapshot,
            config,
            activity,
            ..
        } = &mut *s;
        crate::window_order::sort(order, &snapshot.windows, &config.app_order, activity);
        s.dirty = true;
    }
    render(state);
    let bars: Vec<_> = state
        .borrow()
        .bars
        .iter()
        .map(|bar| (bar.scroll.clone(), bar.root.clone()))
        .collect();
    // Drawing consults State for active feedback; never hold its borrow across AppKit.
    for (scroll, root) in bars {
        let clip = scroll.contentView();
        clip.scrollToPoint(NSPoint::new(0.0, 0.0));
        scroll.reflectScrolledClipView(&clip);
        root.displayIfNeeded();
    }
    state
        .borrow_mut()
        .performance
        .sort
        .record(started.elapsed());
}

pub(super) enum Edit {
    Add(String),
    Remove(String),
    Move(String, bool),
    Tasks,
    Dock,
}
pub(super) fn edit(state: &Shared, edit: Edit) {
    let dock = matches!(edit, Edit::Dock).then(platform::dock_app_order);
    let mut s = state.borrow_mut();
    let scroll_position = s.preferences.as_ref().filter(|p| p.page == 10).map(|p| {
        p.body.frame().size.height
            - p.body.visibleRect().origin.y
            - p.body.visibleRect().size.height
    });
    let mut order = s.config.app_order.clone();
    let mut message = None;
    match edit {
        Edit::Add(bundle) => {
            if !order.contains(&bundle) && order.len() < crate::window_order::MAX_APPS {
                order.push(bundle);
            }
            if let Some(menu) = s.start.take() {
                menu.panel.close();
            }
        }
        Edit::Remove(bundle) => order.retain(|b| b != &bundle),
        Edit::Move(bundle, down) => {
            if let Some(index) = order.iter().position(|b| b == &bundle) {
                let target = if down {
                    index + 1
                } else {
                    index.saturating_sub(1)
                };
                if target < order.len() {
                    order.swap(index, target);
                }
            }
        }
        Edit::Tasks => {
            order.clear();
            for id in &s.order {
                if let Some(w) = s.snapshot.windows.iter().find(|w| w.id == *id)
                    && !w.bundle.is_empty()
                    && !s.config.blacklist.contains(&w.bundle)
                    && !order.contains(&w.bundle)
                {
                    order.push(w.bundle.clone());
                }
            }
            // Keep configured apps that currently have no windows.
            for bundle in &s.config.app_order {
                if !order.contains(bundle) {
                    order.push(bundle.clone());
                }
            }
        }
        Edit::Dock => {
            if let Some(imported) = dock.filter(|apps| !apps.is_empty()) {
                order = imported;
            } else {
                message = Some(
                    "No applications could be read from the Dock. Your order was kept.".into(),
                );
            }
        }
    }
    order.truncate(crate::window_order::MAX_APPS);
    let mut config = s.config.clone();
    config.app_order = order;
    config.app_order_initialized = true;
    config.normalize();
    // Publish the preference change in memory only after the atomic save succeeds.
    let saved = if s.gui_smoke.is_some() {
        Ok(())
    } else {
        config.save()
    };
    match saved {
        Ok(()) => {
            s.config = config;
            s.sort_order_error = message;
        }
        Err(error) => s.sort_order_error = Some(format!("Order could not be saved: {error}")),
    }
    drop(s);
    show_preferences(state, 10);
    if let Some(offset) = scroll_position
        && let Some(p) = &state.borrow().preferences
    {
        p.body
            .scrollPoint(NSPoint::new(0.0, p.body.frame().size.height - offset));
    }
}

pub(super) fn build_editor(state: &Shared, body: &NSView, c: &Config, mut y: f64) {
    let text = label(
        "Main windows by recent activity, then dialogs and utility windows.",
        rect(8.0, y, 730.0, 24.0),
        13.0,
    );
    body.addSubview(&text);
    y -= 26.0;
    let text = label(
        "Apps outside this list follow in their existing order. Changes save automatically.",
        rect(8.0, y, 730.0, 24.0),
        13.0,
    );
    text.setTextColor(Some(&NSColor::secondaryLabelColor()));
    body.addSubview(&text);
    y -= 38.0;
    for (x, width, title, action) in [
        (8.0, 100.0, "Sort now", Action::Sort),
        (120.0, 160.0, "Use taskbar order", Action::UseTaskOrder),
        (292.0, 150.0, "Use Dock order", Action::UseDockOrder),
        (454.0, 140.0, "Add application…", Action::AddOrderApp),
    ] {
        let b = ActionButton::new(state, action, title, rect(x, y, width, 28.0));
        if matches!(b.ivars().action, Action::AddOrderApp) {
            b.setEnabled(c.app_order.len() < crate::window_order::MAX_APPS);
        }
        body.addSubview(&b);
    }
    y -= 36.0;
    if let Some(error) = &state.borrow().sort_order_error {
        let text = label(error, rect(8.0, y, 730.0, 24.0), 12.0);
        text.setTextColor(Some(&NSColor::systemRedColor()));
        body.addSubview(&text);
    }
    y -= 28.0;
    if c.app_order.is_empty() {
        let text = label(
            "Add applications, or copy an order above to get started.",
            rect(8.0, y, 730.0, 26.0),
            13.0,
        );
        body.addSubview(&text);
    }
    for (index, bundle) in c.app_order.iter().enumerate() {
        let name = app_name(state, bundle);
        let rank = label(
            &(index + 1).to_string(),
            rect(8.0, y + 4.0, 28.0, 24.0),
            12.0,
        );
        rank.setTextColor(Some(&NSColor::secondaryLabelColor()));
        rank.setAlignment(NSTextAlignment::Right);
        body.addSubview(&rank);
        let image = NSImageView::initWithFrame(
            NSImageView::alloc(MainThreadMarker::new().unwrap()),
            rect(48.0, y + 4.0, 24.0, 24.0),
        );
        image.setImage(icon(state, bundle, "").as_deref());
        image.setImageScaling(NSImageScaling::ScaleProportionallyUpOrDown);
        body.addSubview(&image);
        let title = label(&name, rect(84.0, y + 4.0, 490.0, 24.0), 13.0);
        title.setUsesSingleLineMode(true);
        title.setLineBreakMode(NSLineBreakMode::ByTruncatingTail);
        title.setToolTip(Some(&NSString::from_str(bundle)));
        body.addSubview(&title);
        for (x, symbol, action, description, enabled) in [
            (
                606.0,
                "chevron.up",
                Action::MoveOrderApp(bundle.clone(), false),
                format!("Move {name} earlier"),
                index > 0,
            ),
            (
                642.0,
                "chevron.down",
                Action::MoveOrderApp(bundle.clone(), true),
                format!("Move {name} later"),
                index + 1 < c.app_order.len(),
            ),
            (
                692.0,
                "minus",
                Action::RemoveOrderApp(bundle.clone()),
                format!("Remove {name} from sort order"),
                true,
            ),
        ] {
            let b = ActionButton::new(state, action, "", rect(x, y + 2.0, 28.0, 28.0));
            b.setBordered(false);
            b.setEnabled(enabled);
            b.setImage(
                NSImage::imageWithSystemSymbolName_accessibilityDescription(
                    &NSString::from_str(symbol),
                    Some(&NSString::from_str(&description)),
                )
                .as_deref(),
            );
            b.setToolTip(Some(&NSString::from_str(&description)));
            // SAFETY: this AppKit button supports the accessibility label setter;
            // the NSString argument is retained for the duration of the call.
            unsafe {
                let _: () =
                    msg_send![&*b, setAccessibilityLabel: &*NSString::from_str(&description)];
            }
            body.addSubview(&b);
        }
        y -= 36.0;
    }
}

fn app_name(state: &Shared, bundle: &str) -> String {
    let s = state.borrow();
    s.apps
        .iter()
        .find(|a| a.bundle == bundle)
        .map(|a| a.name.clone())
        .or_else(|| {
            s.snapshot
                .windows
                .iter()
                .find(|w| w.bundle == bundle)
                .map(|w| w.app.clone())
        })
        .or_else(|| {
            NSWorkspace::sharedWorkspace()
                .URLForApplicationWithBundleIdentifier(&NSString::from_str(bundle))
                .and_then(|url| url.path())
                .map(|path| {
                    std::path::Path::new(&path.to_string())
                        .file_stem()
                        .unwrap_or_default()
                        .to_string_lossy()
                        .into_owned()
                })
        })
        .unwrap_or_else(|| bundle.into())
}
