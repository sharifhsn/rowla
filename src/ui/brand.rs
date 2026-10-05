//! Small, resolution-independent companion to the bundle's app icon.
use super::*;

pub(super) fn status_icon() -> Retained<NSImage> {
    let draw = block2::RcBlock::new(|_bounds: NSRect| {
        NSColor::blackColor().set();
        let window = NSBezierPath::bezierPathWithRoundedRect_xRadius_yRadius(
            rect(1.0, 5.5, 16.0, 11.0),
            2.0,
            2.0,
        );
        window.setLineWidth(1.5);
        window.stroke();
        let divider = NSBezierPath::bezierPath();
        divider.setLineWidth(1.0);
        divider.moveToPoint(NSPoint::new(1.5, 13.0));
        divider.lineToPoint(NSPoint::new(16.5, 13.0));
        divider.stroke();
        for x in [4.0, 6.5, 9.0] {
            NSBezierPath::bezierPathWithOvalInRect(rect(x, 14.2, 1.0, 1.0)).fill();
        }
        for x in [0.5, 6.5, 12.5] {
            NSBezierPath::bezierPathWithRoundedRect_xRadius_yRadius(
                rect(x, 1.0, 5.0, 3.0),
                1.0,
                1.0,
            )
            .fill();
        }
        objc2::runtime::Bool::YES
    });
    let image =
        NSImage::imageWithSize_flipped_drawingHandler(NSSize::new(18.0, 18.0), false, &draw);
    image.setTemplate(true);
    image
}
