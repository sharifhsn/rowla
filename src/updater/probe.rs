//! Read-only local-feed integration test. Never downloads or installs an update.
use super::*;
use objc2::{DefinedClass, MainThreadMarker, MainThreadOnly, define_class};
use objc2_core_foundation::{CFRunLoop, kCFRunLoopDefaultMode};
use objc2_foundation::{NSObject, NSObjectProtocol};
use std::{
    cell::RefCell,
    rc::Rc,
    time::{Duration, Instant},
};
#[derive(Default, serde::Serialize)]
struct ResultData {
    version: Option<String>,
    error: Option<String>,
    finished: bool,
}
struct Ivars {
    result: Rc<RefCell<ResultData>>,
}
define_class!(
    #[unsafe(super = NSObject)]
    #[thread_kind = MainThreadOnly]
    #[ivars = Ivars]
    #[name = "TaskbarRustUpdaterProbe"]
    struct Delegate;

    unsafe impl NSObjectProtocol for Delegate {}

    impl Delegate {
        #[unsafe(method(updater:didFindValidUpdate:))]
        fn found(&self, _updater: &AnyObject, item: &AnyObject) {
            let version: Retained<NSString> = unsafe { msg_send![item, versionString] };
            self.ivars().result.borrow_mut().version = Some(version.to_string());
        }

        #[unsafe(method(updater:didAbortWithError:))]
        fn failed(&self, _updater: &AnyObject, error: &AnyObject) {
            self.ivars().result.borrow_mut().error = Some(native_error(
                std::ptr::from_ref(error).cast_mut(),
                "Sparkle error",
            ));
        }

        #[unsafe(method(updater:didFinishUpdateCycleForUpdateCheck:error:))]
        fn finished(&self, _updater: &AnyObject, _check: isize, error: Option<&AnyObject>) {
            let mut result = self.ivars().result.borrow_mut();
            result.finished = true;
            if let Some(error) = error {
                result.error = Some(native_error(std::ptr::from_ref(error).cast_mut(), "Sparkle error"));
            }
        }
    }
);
pub(super) fn run() -> bool {
    let mtm = MainThreadMarker::new().unwrap();
    let app = objc2_app_kit::NSApplication::sharedApplication(mtm);
    app.setActivationPolicy(objc2_app_kit::NSApplicationActivationPolicy::Accessory);
    let result = Rc::new(RefCell::new(ResultData::default()));
    let delegate: Retained<Delegate> = unsafe {
        msg_send![
            super(Delegate::alloc(mtm).set_ivars(Ivars {
                result: result.clone()
            })),
            init
        ]
    };
    let mut service = Service {
        controller: None,
        _framework: None,
        error: None,
        event: None,
        policy: String::new(),
    };
    if let Err(error) = service.initialize(Some(&delegate), true) {
        println!("{}", serde_json::json!({"error":error,"finished":false}));
        return false;
    }
    unsafe {
        let updater: Retained<AnyObject> =
            msg_send![&**service.controller.as_ref().unwrap(), updater];
        let _: () = msg_send![&*updater, checkForUpdateInformation];
    }
    let start = Instant::now();
    while !result.borrow().finished && start.elapsed() < Duration::from_secs(20) {
        objc2::rc::autoreleasepool(|_| {
            // SAFETY: Core Foundation supplies this immutable run-loop mode.
            CFRunLoop::run_in_mode(unsafe { kCFRunLoopDefaultMode }, 0.05, true);
        });
    }
    let data = result.borrow();
    println!("{}", serde_json::to_string(&*data).unwrap());
    data.finished && data.version.is_some() && data.error.is_none()
}
