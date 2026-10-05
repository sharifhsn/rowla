//! Small preview chrome, with a highlight drawn only around the image.
use super::rect;
use objc2::{AnyThread, DefinedClass, MainThreadOnly, define_class, msg_send, rc::Retained};
use objc2_app_kit::{
    NSBezierPath, NSColor, NSCursor, NSEvent, NSTrackingArea, NSTrackingAreaOptions, NSView,
};
use objc2_foundation::{MainThreadMarker, NSObjectProtocol, NSRect};
use std::cell::{Cell, RefCell};

pub(super) const INSET: f64 = 2.0;

pub(super) fn header_height(font_size: f64) -> f64 {
    (font_size + 6.0).max(20.0)
}

pub(super) struct FrameIvars {
    hovered: Cell<bool>,
    tracking: RefCell<Option<Retained<NSTrackingArea>>>,
}

define_class!(
    #[unsafe(super = NSView)]
    #[thread_kind = MainThreadOnly]
    #[ivars = FrameIvars]
    #[name = "TaskbarRustPreviewFrame"]
    pub(super) struct PreviewFrame;

    unsafe impl NSObjectProtocol for PreviewFrame {}

    impl PreviewFrame {
        #[unsafe(method(drawRect:))]
        fn draw(&self, _dirty: NSRect) {
            let bounds = self.bounds();
            let outline = NSBezierPath::bezierPathWithRoundedRect_xRadius_yRadius(
                rect(0.5, 0.5, (bounds.size.width - 1.0).max(0.0), (bounds.size.height - 1.0).max(0.0)),
                6.0, 6.0,
            );
            NSColor::windowBackgroundColor().set();
            outline.fill();
            if self.ivars().hovered.get() {
                NSColor::labelColor().colorWithAlphaComponent(0.06).set();
                outline.fill();
                NSColor::controlAccentColor().colorWithAlphaComponent(0.3).set();
            } else {
                NSColor::separatorColor().colorWithAlphaComponent(0.5).set();
            }
            outline.setLineWidth(1.0);
            outline.stroke();
        }

        #[unsafe(method(mouseEntered:))]
        fn entered(&self, _event: &NSEvent) {
            self.ivars().hovered.set(true);
            self.setNeedsDisplay(true);
            NSCursor::arrowCursor().set();
        }

        #[unsafe(method(mouseExited:))]
        fn exited(&self, _event: &NSEvent) {
            self.clear_highlight();
        }

        #[unsafe(method(mouseMoved:))]
        fn moved(&self, _event: &NSEvent) {
            NSCursor::arrowCursor().set();
        }

        #[unsafe(method(cursorUpdate:))]
        fn cursor_update(&self, _event: &NSEvent) {
            NSCursor::arrowCursor().set();
        }

        #[unsafe(method(resetCursorRects))]
        fn reset_cursor_rects(&self) {
            self.addCursorRect_cursor(self.visibleRect(), &NSCursor::arrowCursor());
        }

        #[unsafe(method(mouseDownCanMoveWindow))]
        fn mouse_down_can_move_window(&self) -> bool { false }

        #[unsafe(method(updateTrackingAreas))]
        fn update_tracking_areas(&self) {
            if let Some(area) = self.ivars().tracking.borrow_mut().take() {
                self.removeTrackingArea(&area);
            }
            // SAFETY: the owner is this live NSView and implements the mouse selectors.
            // InVisibleRect lets AppKit follow layout; exactly one area is retained.
            let area = unsafe {
                NSTrackingArea::initWithRect_options_owner_userInfo(
                    NSTrackingArea::alloc(), self.bounds(),
                    NSTrackingAreaOptions::MouseEnteredAndExited
                        | NSTrackingAreaOptions::MouseMoved
                        | NSTrackingAreaOptions::ActiveAlways
                        | NSTrackingAreaOptions::InVisibleRect,
                    Some(self), None,
                )
            };
            self.addTrackingArea(&area);
            *self.ivars().tracking.borrow_mut() = Some(area);
            unsafe { let _: () = msg_send![super(self), updateTrackingAreas]; }
        }
    }
);

impl PreviewFrame {
    pub(super) fn new(frame: NSRect) -> Retained<Self> {
        let mtm = MainThreadMarker::new().unwrap();
        unsafe {
            msg_send![super(Self::alloc(mtm).set_ivars(FrameIvars {
                hovered: Cell::new(false), tracking: RefCell::new(None),
            })), initWithFrame: frame]
        }
    }

    pub(super) fn clear_highlight(&self) {
        if self.ivars().hovered.replace(false) {
            self.setNeedsDisplay(true);
        }
    }

    pub(super) fn hovered(&self) -> bool {
        self.ivars().hovered.get()
    }
}
