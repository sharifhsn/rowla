//! Optional newer AppKit/ScreenCaptureKit APIs. The base app runs on macOS 15.2.
use objc2::{msg_send, runtime::AnyClass, sel};
use objc2_app_kit::NSView;
use objc2_foundation::{NSObjectProtocol, NSProcessInfo};
use objc2_screen_capture_kit::SCStream;

pub(crate) fn compact_controls(view: &NSView) {
    if view.respondsToSelector(sel!(setPrefersCompactControlSizeMetrics:)) {
        // SAFETY: this selector exists only on macOS 26+. NSView is main-thread
        // confined, and the checked setter takes one Objective-C BOOL.
        view.setPrefersCompactControlSizeMetrics(true);
    }
}

pub(crate) fn native_capture_state(stream: &SCStream) -> Option<bool> {
    if stream.respondsToSelector(sel!(isCapturing)) {
        // SAFETY: macOS 27 implements this BOOL getter. The serial capture
        // worker owns the stream; this observation never authorizes disposal.
        Some(unsafe { msg_send![stream, isCapturing] })
    } else {
        None
    }
}

pub(crate) fn compatibility_probe() -> bool {
    let required = [
        (c"NSApplication", sel!(yieldActivationToApplication:), false),
        (
            c"NSRunningApplication",
            sel!(activateFromApplication:options:),
            false,
        ),
        (
            c"SCContentFilter",
            sel!(initWithDesktopIndependentWindow:),
            false,
        ),
        (
            c"SCStreamConfiguration",
            sel!(setIgnoreShadowsSingleWindow:),
            false,
        ),
        (c"SCStream", sel!(startCaptureWithCompletionHandler:), false),
        (
            c"SCScreenshotManager",
            sel!(captureImageWithFilter:configuration:completionHandler:),
            true,
        ),
        (c"SMAppService", sel!(mainAppService), true),
    ];
    let mut passed = true;
    let mut checks = Vec::new();
    for (name, selector, class_method) in required {
        let available = AnyClass::get(name).is_some_and(|class| {
            // SAFETY: NSObject class methods inspect selectors without creating
            // capture objects, accessing other apps, or requesting permissions.
            unsafe {
                if class_method {
                    msg_send![class, respondsToSelector:selector]
                } else {
                    msg_send![class, instancesRespondToSelector:selector]
                }
            }
        });
        passed &= available;
        checks.push(serde_json::json!({"class":name.to_string_lossy(), "selector":selector.name().to_string_lossy(), "available":available}));
    }
    println!(
        "{}",
        serde_json::json!({"passed":passed,"minimum_macos":"15.2",
        "os":NSProcessInfo::processInfo().operatingSystemVersionString().to_string(),"checks":checks})
    );
    passed
}
