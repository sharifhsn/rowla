//! Sparkle 2 owns appcast verification, staging, installation and recovery.
//! All controller access stays on AppKit's main thread.
use objc2::{
    AnyThread, msg_send,
    rc::Retained,
    runtime::{AnyClass, AnyObject},
};
use objc2_foundation::{NSBundle, NSString};
mod probe;
pub(crate) fn probe() -> bool {
    probe::run()
}
pub(crate) enum Event {
    Checked(Result<bool, String>),
}
pub(crate) struct Service {
    controller: Option<Retained<AnyObject>>,
    _framework: Option<Retained<NSBundle>>,
    error: Option<String>,
    event: Option<Event>,
    policy: String,
}
impl Service {
    pub(crate) fn new() -> Self {
        let mut this = Self {
            controller: None,
            _framework: None,
            error: None,
            event: None,
            policy: String::new(),
        };
        if let Err(error) = this.initialize(None, false) {
            this.error = Some(error);
        }
        this
    }
    fn initialize(
        &mut self,
        delegate: Option<&AnyObject>,
        local_probe: bool,
    ) -> Result<(), String> {
        if objc2::MainThreadMarker::new().is_none() {
            return Err("Sparkle requires the application main thread".into());
        }
        let bundle = NSBundle::mainBundle();
        let value = |key: &str| {
            bundle
                .objectForInfoDictionaryKey(&NSString::from_str(key))
                .and_then(|v| v.downcast_ref::<NSString>().map(|s| s.to_string()))
                .unwrap_or_default()
        };
        let feed = value("SUFeedURL");
        let public = value("SUPublicEDKey");
        if local_probe
            && (!value("SUDefaultsDomain").starts_with("io.sharif.taskbarrust.probe.")
                || !local_feed(&feed))
        {
            return Err("The information probe requires an isolated test defaults domain and a loopback feed; it cannot change production update preferences.".into());
        }
        if !(valid_feed(&feed) || local_probe && local_feed(&feed)) {
            return Err("Updates are not configured: the distributor must set an HTTPS appcast URL and an Ed25519 public key.".into());
        }
        let key = objc2_foundation::NSData::initWithBase64EncodedString_options(
            objc2_foundation::NSData::alloc(),
            &NSString::from_str(&public),
            objc2_foundation::NSDataBase64DecodingOptions::empty(),
        );
        if key.is_none_or(|k| k.length() != 32) {
            return Err("Invalid Sparkle Ed25519 public key".into());
        }
        let path = std::path::PathBuf::from(bundle.bundlePath().to_string())
            .join("Contents/Frameworks/Sparkle.framework");
        let framework = NSBundle::bundleWithPath(&NSString::from_str(&path.to_string_lossy()))
            .ok_or("Sparkle framework is missing")?;
        let mut error: *mut AnyObject = std::ptr::null_mut();
        let loaded: bool = unsafe { msg_send![&*framework,loadAndReturnError:&mut error] };
        if !loaded {
            return Err(native_error(error, "Could not load Sparkle"));
        }
        let class = AnyClass::get(c"SPUStandardUpdaterController")
            .ok_or("Sparkle controller is unavailable")?;
        let allocated: objc2::rc::Allocated<AnyObject> = unsafe { msg_send![class, alloc] };
        let controller: Retained<AnyObject> = unsafe {
            msg_send![allocated,initWithStartingUpdater:false,updaterDelegate:delegate,userDriverDelegate:std::ptr::null::<AnyObject>()]
        };
        let updater: Retained<AnyObject> = unsafe { msg_send![&*controller, updater] };
        if local_probe {
            // An explicit diagnostic must not inherit scheduled checks/downloads.
            unsafe {
                let _: () = msg_send![&*updater,setAutomaticallyChecksForUpdates:false];
                let _: () = msg_send![&*updater,setAutomaticallyDownloadsUpdates:false];
            }
        }
        let started: bool = unsafe { msg_send![&*updater,startUpdater:&mut error] };
        if !started {
            return Err(native_error(error, "Sparkle could not start"));
        }
        self._framework = Some(framework);
        self.controller = Some(controller);
        Ok(())
    }
    pub(crate) fn policy(&mut self, policy: &str) {
        if self.policy == policy {
            return;
        }
        self.policy = policy.into();
        if let Some(controller) = &self.controller {
            unsafe {
                let updater: Retained<AnyObject> = msg_send![&**controller, updater];
                let _: () = msg_send![&*updater,setAutomaticallyChecksForUpdates:policy!="manual"];
                let _: () =
                    msg_send![&*updater,setAutomaticallyDownloadsUpdates:policy=="automatic"];
            }
        }
    }
    pub(crate) fn check(&mut self) -> bool {
        if let Some(error) = &self.error {
            self.event = Some(Event::Checked(Err(error.clone())));
            return false;
        }
        let Some(controller) = &self.controller else {
            return false;
        };
        unsafe {
            let updater: Retained<AnyObject> = msg_send![&**controller, updater];
            let can: bool = msg_send![&*updater, canCheckForUpdates];
            if can {
                let _: () = msg_send![&**controller,checkForUpdates:std::ptr::null::<AnyObject>()];
            }
            can
        }
    }
    pub(crate) fn poll(&mut self) -> Option<Event> {
        self.event.take()
    }
}
fn native_error(error: *mut AnyObject, fallback: &str) -> String {
    if error.is_null() {
        fallback.into()
    } else {
        let text: Retained<NSString> = unsafe { msg_send![error, localizedDescription] };
        text.to_string()
    }
}
fn valid_feed(feed: &str) -> bool {
    let Some(url) = objc2_foundation::NSURL::URLWithString(&NSString::from_str(feed)) else {
        return false;
    };
    url.scheme().is_some_and(|s| s.to_string() == "https")
        && url.host().is_some_and(|s| !s.is_empty())
        && url.user().is_none()
        && url.password().is_none()
}
fn local_feed(feed: &str) -> bool {
    let Some(url) = objc2_foundation::NSURL::URLWithString(&NSString::from_str(feed)) else {
        return false;
    };
    url.scheme().is_some_and(|s| s.to_string() == "http")
        && url.host().is_some_and(|s| s.to_string() == "127.0.0.1")
        && url.user().is_none()
        && url.password().is_none()
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn release_feeds_require_https_and_no_credentials() {
        for url in [
            "",
            "http://example.com/feed.xml",
            "https://secret@example.com/feed.xml",
            "https:",
        ] {
            assert!(!valid_feed(url));
        }
        assert!(valid_feed("https://example.com/taskbar/appcast.xml"));
    }
}
