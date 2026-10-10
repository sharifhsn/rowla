//! Pointer-scoped Command-W/Q. No key contents, history, timers or capture work.
use super::*;
use objc2_core_foundation::{
    CFMachPort, CFRetained, CFRunLoop, CFRunLoopSource, kCFRunLoopCommonModes,
};
use objc2_core_graphics::{
    CGEvent, CGEventField, CGEventFlags, CGEventTapLocation, CGEventTapOptions,
    CGEventTapPlacement, CGEventTapProxy, CGEventType,
};
use std::{ffi::c_void, panic::AssertUnwindSafe, ptr::NonNull};

const W_KEY: i64 = 13;
const Q_KEY: i64 = 12;

#[derive(Default)]
pub(super) struct KeyState {
    consumed: [bool; 2],
}
impl KeyState {
    pub(super) fn decision(
        &mut self,
        kind: CGEventType,
        key: i64,
        flags: CGEventFlags,
        repeat: bool,
        target: Option<u32>,
    ) -> Decision {
        let slot = match key {
            W_KEY => 0,
            Q_KEY => 1,
            _ => return Decision::Pass,
        };
        if kind == CGEventType::KeyUp {
            return if std::mem::take(&mut self.consumed[slot]) {
                Decision::Consume
            } else {
                Decision::Pass
            };
        }
        if repeat {
            return if self.consumed[slot] {
                Decision::Consume
            } else {
                Decision::Pass
            };
        }
        self.consumed[slot] = false;
        let modifiers = flags
            & (CGEventFlags::MaskCommand
                | CGEventFlags::MaskShift
                | CGEventFlags::MaskAlternate
                | CGEventFlags::MaskControl);
        if kind == CGEventType::KeyDown
            && modifiers == CGEventFlags::MaskCommand
            && let Some(id) = target
        {
            self.consumed[slot] = true;
            if key == Q_KEY {
                Decision::Quit(id)
            } else {
                Decision::Close(id)
            }
        } else {
            Decision::Pass
        }
    }
}

#[derive(Debug, PartialEq, Eq)]
pub(super) enum Decision {
    Pass,
    Consume,
    Close(u32),
    Quit(u32),
}
struct Context {
    state: WeakState,
    keys: RefCell<KeyState>,
    tap: Cell<Option<NonNull<CFMachPort>>>,
}
pub(super) struct Service {
    tap: CFRetained<CFMachPort>,
    source: CFRetained<CFRunLoopSource>,
    run_loop: CFRetained<CFRunLoop>,
    context: Box<Context>,
}
impl Service {
    fn new(state: &Shared) -> Result<Self, String> {
        MainThreadMarker::new().ok_or("Hover shortcuts must run on the main thread")?;
        let mut context = Box::new(Context {
            state: Rc::downgrade(state),
            keys: RefCell::new(KeyState::default()),
            tap: Cell::new(None),
        });
        // SAFETY: the boxed context stays at this address until Drop removes
        // the source and invalidates the tap. Its callback runs only on the
        // main run loop, so AppKit and the weak Rc state stay on their thread.
        let tap = unsafe {
            CGEvent::tap_create(
                CGEventTapLocation::SessionEventTap,
                CGEventTapPlacement::HeadInsertEventTap,
                CGEventTapOptions::Default,
                (1 << CGEventType::KeyDown.0) | (1 << CGEventType::KeyUp.0),
                Some(callback),
                (&mut *context as *mut Context).cast(),
            )
        }
        .ok_or("Hover shortcuts unavailable. Check Rowla's Accessibility access")?;
        let source = CFMachPort::new_run_loop_source(None, Some(&tap), 0)
            .ok_or("Could not create the hover shortcut event source")?;
        let run_loop = CFRunLoop::main().ok_or("Main run loop unavailable")?;
        context.tap.set(Some(NonNull::from(&*tap)));
        // SAFETY: CoreFoundation's common-modes constant is immutable.
        run_loop.add_source(Some(&source), unsafe { kCFRunLoopCommonModes });
        Ok(Self {
            tap,
            source,
            run_loop,
            context,
        })
    }
    pub(super) fn enabled(&self) -> bool {
        CGEvent::tap_is_enabled(&self.tap)
    }
}
impl Drop for Service {
    fn drop(&mut self) {
        CGEvent::tap_enable(&self.tap, false);
        // SAFETY: the same immutable mode used when adding this source.
        self.run_loop
            .remove_source(Some(&self.source), unsafe { kCFRunLoopCommonModes });
        self.source.invalidate();
        self.tap.invalidate();
        self.context.tap.set(None);
    }
}

pub(super) fn install(state: &Shared) {
    if state.borrow().gui_smoke.is_some() || state.borrow().hover_close.is_some() {
        return;
    }
    match Service::new(state) {
        Ok(service) => state.borrow_mut().hover_close = Some(service),
        Err(error) => state.borrow_mut().error = error,
    }
}

unsafe extern "C-unwind" fn callback(
    _proxy: CGEventTapProxy,
    kind: CGEventType,
    event: NonNull<CGEvent>,
    user_info: *mut c_void,
) -> *mut CGEvent {
    // SAFETY: Service owns this boxed context and invalidates the main-loop
    // callback before freeing it. CoreGraphics supplies a valid event.
    let context = unsafe { &*user_info.cast::<Context>() };
    if kind == CGEventType::TapDisabledByTimeout {
        if let Some(tap) = context.tap.get() {
            // SAFETY: Service still owns the tap while its callback executes.
            CGEvent::tap_enable(unsafe { tap.as_ref() }, true);
        }
        context.keys.borrow_mut().consumed = [false; 2];
        return event.as_ptr();
    }
    if kind == CGEventType::TapDisabledByUserInput {
        context.keys.borrow_mut().consumed = [false; 2];
        return event.as_ptr();
    }
    // SAFETY: CoreGraphics keeps the callback's event alive until it returns.
    let key_event = unsafe { event.as_ref() };
    let key = CGEvent::integer_value_field(Some(key_event), CGEventField::KeyboardEventKeycode);
    if !matches!(key, W_KEY | Q_KEY) {
        return event.as_ptr();
    }
    std::panic::catch_unwind(AssertUnwindSafe(|| {
        autoreleasepool(|_| {
            let flags = CGEvent::flags(Some(key_event));
            let repeat = CGEvent::integer_value_field(
                Some(key_event),
                CGEventField::KeyboardEventAutorepeat,
            ) != 0;
            let Some(state) = context.state.upgrade() else {
                return event.as_ptr();
            };
            let target = if kind == CGEventType::KeyDown
                && !repeat
                && flags.contains(CGEventFlags::MaskCommand)
            {
                let point = NSEvent::mouseLocation();
                let frontmost = NSWindow::windowNumberAtPoint_belowWindowWithWindowNumber(
                    point,
                    0,
                    MainThreadMarker::new().expect("event tap runs on the main loop"),
                );
                state
                    .try_borrow()
                    .ok()
                    .and_then(|s| action_at(&s, point, frontmost, key))
            } else {
                None
            };
            match context.keys.borrow_mut().decision(
                kind,
                key,
                flags,
                repeat,
                target.as_ref().map(|_| 1),
            ) {
                Decision::Pass => event.as_ptr(),
                Decision::Consume => std::ptr::null_mut(),
                Decision::Close(_) | Decision::Quit(_) => {
                    // Consume even if the queue rejects the command: falling
                    // through would close or quit the unrelated focused app.
                    if let Some(action) = target {
                        dispatch(&state, action);
                    }
                    std::ptr::null_mut()
                }
            }
        })
    }))
    .unwrap_or(event.as_ptr())
}

pub(super) fn action_at(s: &State, point: NSPoint, frontmost: isize, key: i64) -> Option<Action> {
    let close = if let Some(id) = target_at(s, point, frontmost) {
        Some(Action::Close(id))
    } else {
        s.bars
            .iter()
            .filter(|b| b.panel.isVisible() && b.panel.windowNumber() == frontmost)
            .find_map(|bar| {
                let local = bar.panel.convertPointFromScreen(point);
                let hit = bar
                    .root
                    .hitTest(bar.root.convertPoint_fromView(local, None))?;
                let button = hit.downcast_ref::<ActionButton>()?;
                if let Action::Tab(id, index) = button.ivars().action
                    && button.isEnabled()
                    && s.snapshot
                        .windows
                        .iter()
                        .find(|w| w.id == id)
                        .is_some_and(|w| w.tabs.iter().any(|t| t.id == index))
                {
                    Some(Action::CloseTab(id, index))
                } else {
                    None
                }
            })
    }?;
    match key {
        W_KEY => Some(close),
        Q_KEY => {
            let id = match close {
                Action::Close(id) | Action::CloseTab(id, _) => id,
                _ => return None,
            };
            s.snapshot
                .windows
                .iter()
                .find(|w| w.id == id && w.pid > 0)
                .map(|w| Action::QuitApp(w.pid))
        }
        _ => None,
    }
}

pub(super) fn tile_at(bar: &Bar, point: NSPoint) -> Option<u32> {
    let local = bar.panel.convertPointFromScreen(point);
    let hit = bar
        .root
        .hitTest(bar.root.convertPoint_fromView(local, None))?;
    let button = hit.downcast_ref::<ActionButton>()?;
    match button.ivars().action {
        Action::Window(id) | Action::Bubble(id) if button.isEnabled() => Some(id),
        _ => None,
    }
}
pub(super) fn target_at(s: &State, point: NSPoint, frontmost: isize) -> Option<u32> {
    let alive = |id: &u32| s.snapshot.windows.iter().any(|w| w.id == *id);
    if let Some(preview) = &s.preview
        && preview.panel.isVisible()
        && preview.panel.windowNumber() == frontmost
        && contains(preview.panel.frame(), point)
    {
        return s.preview_id.filter(alive);
    }
    s.bars.iter().find_map(|bar| {
        (bar.panel.isVisible() && bar.panel.windowNumber() == frontmost)
            .then(|| tile_at(bar, point))
            .flatten()
            .filter(alive)
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn close_is_pointer_scoped_and_other_chords_pass_through() {
        let mut keys = KeyState::default();
        let cmd = CGEventFlags::MaskCommand;
        assert_eq!(
            keys.decision(CGEventType::KeyDown, W_KEY, cmd, false, None),
            Decision::Pass
        );
        for modifiers in [
            CGEventFlags::empty(),
            cmd | CGEventFlags::MaskShift,
            cmd | CGEventFlags::MaskAlternate,
            cmd | CGEventFlags::MaskControl,
        ] {
            assert_eq!(
                keys.decision(CGEventType::KeyDown, W_KEY, modifiers, false, Some(7)),
                Decision::Pass
            );
        }
        assert_eq!(
            keys.decision(CGEventType::KeyDown, 17, cmd, false, Some(7)),
            Decision::Pass
        );
        assert_eq!(
            keys.decision(CGEventType::KeyDown, W_KEY, cmd, false, Some(7)),
            Decision::Close(7)
        );
        assert_eq!(
            keys.decision(
                CGEventType::KeyUp,
                W_KEY,
                CGEventFlags::empty(),
                false,
                None
            ),
            Decision::Consume
        );
        assert_eq!(
            keys.decision(CGEventType::KeyUp, W_KEY, cmd, false, None),
            Decision::Pass
        );
    }
    #[test]
    fn holding_close_cannot_close_the_neighbor_or_focused_app() {
        let mut keys = KeyState::default();
        let cmd = CGEventFlags::MaskCommand;
        assert_eq!(
            keys.decision(CGEventType::KeyDown, W_KEY, cmd, false, Some(7)),
            Decision::Close(7)
        );
        for target in [Some(8), None] {
            assert_eq!(
                keys.decision(CGEventType::KeyDown, W_KEY, cmd, true, target),
                Decision::Consume
            );
        }
        keys.decision(CGEventType::KeyUp, W_KEY, cmd, false, None);
        assert_eq!(
            keys.decision(CGEventType::KeyDown, W_KEY, cmd, true, Some(8)),
            Decision::Pass
        );
        assert_eq!(
            keys.decision(CGEventType::KeyDown, W_KEY, cmd, false, Some(8)),
            Decision::Close(8)
        );
    }
    #[test]
    fn quit_is_pointer_scoped_and_passes_other_chords_and_unclaimed_repeats() {
        let mut keys = KeyState::default();
        let cmd = CGEventFlags::MaskCommand;
        assert_eq!(
            keys.decision(CGEventType::KeyDown, Q_KEY, cmd, false, None),
            Decision::Pass
        );
        assert_eq!(
            keys.decision(CGEventType::KeyDown, Q_KEY, cmd, true, Some(7)),
            Decision::Pass
        );
        for flags in [
            CGEventFlags::empty(),
            cmd | CGEventFlags::MaskShift,
            cmd | CGEventFlags::MaskAlternate,
            cmd | CGEventFlags::MaskControl,
        ] {
            assert_eq!(
                keys.decision(CGEventType::KeyDown, Q_KEY, flags, false, Some(7)),
                Decision::Pass
            );
        }
        assert_eq!(
            keys.decision(CGEventType::KeyDown, Q_KEY, cmd, false, Some(7)),
            Decision::Quit(7)
        );
        for target in [Some(8), None] {
            assert_eq!(
                keys.decision(CGEventType::KeyDown, Q_KEY, cmd, true, target),
                Decision::Consume
            );
        }
        assert_eq!(
            keys.decision(
                CGEventType::KeyUp,
                Q_KEY,
                CGEventFlags::empty(),
                false,
                None
            ),
            Decision::Consume
        );
        assert_eq!(
            keys.decision(CGEventType::KeyUp, Q_KEY, cmd, false, None),
            Decision::Pass
        );
    }
    #[test]
    fn close_and_quit_track_their_own_key_releases() {
        let mut keys = KeyState::default();
        let cmd = CGEventFlags::MaskCommand;
        assert_eq!(
            keys.decision(CGEventType::KeyDown, W_KEY, cmd, false, Some(7)),
            Decision::Close(7)
        );
        assert_eq!(
            keys.decision(CGEventType::KeyDown, Q_KEY, cmd, false, Some(8)),
            Decision::Quit(8)
        );
        assert_eq!(
            keys.decision(CGEventType::KeyUp, Q_KEY, cmd, false, None),
            Decision::Consume
        );
        assert_eq!(
            keys.decision(CGEventType::KeyDown, W_KEY, cmd, true, None),
            Decision::Consume
        );
        assert_eq!(
            keys.decision(CGEventType::KeyUp, W_KEY, cmd, false, None),
            Decision::Consume
        );
    }
}
