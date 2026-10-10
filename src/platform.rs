//! macOS window service. All AX elements belong to this worker; the UI receives values.
//! Every Core Foundation Create/Copy result has exactly one owner and release.
use crate::config::Config;
use crate::ipc_budget;
use objc2::{
    msg_send,
    rc::Retained,
    rc::autoreleasepool,
    runtime::{AnyClass, AnyObject},
};
use objc2_app_kit::{NSApplicationActivationOptions, NSRunningApplication, NSWorkspace};
use objc2_foundation::{NSArray, NSPoint, NSSize, NSString, NSURL, ns_string};
use std::{
    collections::{HashMap, HashSet},
    ffi::{c_char, c_void},
    sync::mpsc::{Receiver, Sender},
    time::{Duration, Instant},
};

type Ref = *const c_void;
#[link(name = "CoreFoundation", kind = "framework")]
unsafe extern "C" {
    fn CFRelease(value: Ref);
    fn CFRetain(value: Ref) -> Ref;
    fn CFGetTypeID(value: Ref) -> usize;
    fn CFStringGetTypeID() -> usize;
    fn CFNumberGetTypeID() -> usize;
    fn CFBooleanGetTypeID() -> usize;
    fn CFDictionaryGetTypeID() -> usize;
    fn CFArrayGetTypeID() -> usize;
    fn CFEqual(a: Ref, b: Ref) -> bool;
    fn CFStringGetCString(value: Ref, buffer: *mut c_char, size: isize, encoding: u32) -> bool;
    fn CFStringGetLength(value: Ref) -> isize;
    fn CFArrayGetCount(value: Ref) -> isize;
    fn CFArrayGetValueAtIndex(value: Ref, index: isize) -> Ref;
    fn CFArrayCreate(allocator: Ref, values: *const Ref, count: isize, callbacks: Ref) -> Ref;
    static kCFTypeArrayCallBacks: [usize; 5];
    fn CFDictionaryGetValue(dict: Ref, key: Ref) -> Ref;
    fn CFNumberGetValue(value: Ref, kind: isize, output: *mut c_void) -> bool;
    fn CFNumberCreate(allocator: Ref, kind: isize, value: *const c_void) -> Ref;
    fn CFBooleanGetValue(value: Ref) -> bool;
    static kCFBooleanTrue: Ref;
    static kCFBooleanFalse: Ref;
}
#[link(name = "ApplicationServices", kind = "framework")]
unsafe extern "C" {
    fn AXIsProcessTrusted() -> bool;
    fn AXIsProcessTrustedWithOptions(options: Ref) -> bool;
    fn AXUIElementCreateApplication(pid: i32) -> Ref;
    fn AXUIElementGetTypeID() -> usize;
    fn AXValueGetTypeID() -> usize;
    fn AXUIElementSetMessagingTimeout(element: Ref, timeout: f32) -> i32;
    fn AXUIElementCopyAttributeValue(element: Ref, name: Ref, out: *mut Ref) -> i32;
    fn AXUIElementSetAttributeValue(element: Ref, name: Ref, value: Ref) -> i32;
    fn AXUIElementPerformAction(element: Ref, action: Ref) -> i32;
    fn AXValueCreate(kind: u32, value: *const c_void) -> Ref;
    fn AXValueGetValue(value: Ref, kind: u32, output: *mut c_void) -> bool;
    fn CGWindowListCopyWindowInfo(options: u32, relative: u32) -> Ref;
    fn CGPreflightScreenCaptureAccess() -> bool;
    fn CGRequestScreenCaptureAccess() -> bool;
}
#[link(name = "ScreenCaptureKit", kind = "framework")]
unsafe extern "C" {}
#[link(name = "ServiceManagement", kind = "framework")]
unsafe extern "C" {}

struct Owned(Ref);
struct TabElement {
    element: Owned,
    owner: Owned,
    seen: Instant,
}
static NEXT_TAB_ID: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(1);
static CF_LIVE: std::sync::atomic::AtomicUsize = std::sync::atomic::AtomicUsize::new(0);
static CF_PEAK: std::sync::atomic::AtomicUsize = std::sync::atomic::AtomicUsize::new(0);
fn owned(p: Ref) -> Owned {
    let n = CF_LIVE.fetch_add(1, std::sync::atomic::Ordering::Relaxed) + 1;
    CF_PEAK.fetch_max(n, std::sync::atomic::Ordering::Relaxed);
    Owned(p)
}
pub fn cf_counts() -> (usize, usize) {
    (
        CF_LIVE.load(std::sync::atomic::Ordering::Relaxed),
        CF_PEAK.load(std::sync::atomic::Ordering::Relaxed),
    )
}
impl Owned {
    unsafe fn from_create(p: Ref) -> Option<Self> {
        if p.is_null() { None } else { Some(owned(p)) }
    }
    unsafe fn from_borrowed(p: Ref) -> Option<Self> {
        if p.is_null() {
            None
        } else {
            Some(owned(unsafe { CFRetain(p) }))
        }
    }
}
impl Drop for Owned {
    fn drop(&mut self) {
        CF_LIVE.fetch_sub(1, std::sync::atomic::Ordering::Relaxed);
        unsafe { CFRelease(self.0) }
    }
}
fn cf_string(name: &NSString) -> Ref {
    let string: &objc2_core_foundation::CFString = name.as_ref();
    std::ptr::from_ref(string).cast()
}
fn string(p: Ref) -> String {
    if p.is_null() {
        return String::new();
    }
    unsafe {
        if CFGetTypeID(p) != CFStringGetTypeID() {
            return String::new();
        }
        let mut data = vec![0u8; (CFStringGetLength(p) as usize * 4 + 1).min(32769)];
        if CFStringGetCString(p, data.as_mut_ptr().cast(), data.len() as isize, 0x08000100) {
            std::ffi::CStr::from_ptr(data.as_ptr().cast())
                .to_string_lossy()
                .into_owned()
        } else {
            String::new()
        }
    }
}
fn number(p: Ref) -> i64 {
    let mut v = 0i64;
    if !p.is_null() && unsafe { CFGetTypeID(p) == CFNumberGetTypeID() } {
        unsafe {
            CFNumberGetValue(p, 4, (&mut v as *mut i64).cast());
        }
    }
    v
}
fn dict(d: Ref, key: &NSString) -> Ref {
    if d.is_null() || unsafe { CFGetTypeID(d) != CFDictionaryGetTypeID() } {
        return std::ptr::null();
    }
    unsafe { CFDictionaryGetValue(d, cf_string(key)) }
}
fn array(value: &Owned) -> impl ExactSizeIterator<Item = Ref> + DoubleEndedIterator + '_ {
    // SAFETY: Owned retains the native value for this iterator's lifetime.
    // Check its type before indexing, and preserve the native enumeration limit.
    let count = unsafe {
        if CFGetTypeID(value.0) == CFArrayGetTypeID() {
            CFArrayGetCount(value.0).clamp(0, 4096)
        } else {
            0
        }
    };
    (0..count).map(move |index| unsafe { CFArrayGetValueAtIndex(value.0, index) })
}
fn attr(el: Ref, name: &NSString) -> Option<Owned> {
    if el.is_null() || unsafe { CFGetTypeID(el) != AXUIElementGetTypeID() } {
        crate::ipc_budget::record(-25201, false);
        return None;
    }
    let timeout = crate::ipc_budget::timeout()?;
    let mut out = std::ptr::null();
    unsafe {
        AXUIElementSetMessagingTimeout(el, timeout);
        let error = AXUIElementCopyAttributeValue(el, cf_string(name), &mut out);
        crate::ipc_budget::record(error, true);
        if error == 0 {
            Owned::from_create(out)
        } else {
            // Defensive ownership if an implementation supplies a value on error.
            drop(Owned::from_create(out));
            None
        }
    }
}
fn text_attr(el: Ref, name: &NSString) -> String {
    attr(el, name).map(|v| string(v.0)).unwrap_or_default()
}
fn bool_attr(el: Ref, name: &NSString) -> bool {
    attr(el, name)
        .map(|v| unsafe { CFGetTypeID(v.0) == CFBooleanGetTypeID() && CFBooleanGetValue(v.0) })
        .unwrap_or(false)
}
fn set_bool(el: Ref, name: &NSString, value: bool) -> bool {
    set_bool_tracking(el, name, value, true)
}
fn set_bool_tracking(el: Ref, name: &NSString, value: bool, required: bool) -> bool {
    let Some(timeout) = crate::ipc_budget::timeout() else {
        return false;
    };
    unsafe {
        AXUIElementSetMessagingTimeout(el, timeout);
        let error = AXUIElementSetAttributeValue(
            el,
            cf_string(name),
            if value {
                kCFBooleanTrue
            } else {
                kCFBooleanFalse
            },
        );
        if required {
            crate::ipc_budget::record(error, false);
        }
        error == 0
    }
}
fn action(el: Ref, name: &NSString) -> bool {
    let Some(timeout) = crate::ipc_budget::timeout() else {
        return false;
    };
    unsafe {
        AXUIElementSetMessagingTimeout(el, timeout);
        let error = AXUIElementPerformAction(el, cf_string(name));
        crate::ipc_budget::record(error, false);
        error == 0
    }
}

pub(crate) use crate::models::{Application, Snapshot, Window};
#[derive(Clone, Debug)]
pub enum Command {
    Activate(u32, bool),
    SelectTab(u64),
    CloseTab(u64),
    CloseSelectedTab(u64),
    Minimize(u32),
    Close(u32),
    CloseRestored(u32),
    Fullscreen(u32),
    Hide(i32),
    Quit(i32),
    Resize(u32, f64),
    Dock(bool),
    Launch(String, String),
    Probe(std::sync::mpsc::Sender<Instant>),
    Stop,
}
pub fn trusted() -> bool {
    unsafe { AXIsProcessTrusted() }
}
pub fn request_accessibility() -> bool {
    use objc2_foundation::{NSDictionary, NSNumber};
    let key = NSString::from_str("AXTrustedCheckOptionPrompt");
    let value = NSNumber::new_bool(true);
    let options = NSDictionary::from_slices(&[&*key], &[&*value]);
    unsafe {
        AXIsProcessTrustedWithOptions((&*options as *const NSDictionary<NSString, NSNumber>).cast())
    }
}
pub fn screen_allowed() -> bool {
    unsafe { CGPreflightScreenCaptureAccess() }
}
pub fn request_screen() -> bool {
    unsafe { CGRequestScreenCaptureAccess() }
}

fn active_spaces() -> HashSet<i64> {
    let Some(data) = (unsafe { Owned::from_create(crate::private_api::active_spaces()) }) else {
        return HashSet::new();
    };
    array(&data)
        .map(|d| {
            number(dict(
                dict(d, ns_string!("Current Space")),
                ns_string!("ManagedSpaceID"),
            ))
        })
        .filter(|v| *v > 0)
        .collect()
}
fn window_spaces(id: u32) -> Vec<i64> {
    let n = id as i64;
    let Some(num) = (unsafe {
        Owned::from_create(CFNumberCreate(
            std::ptr::null(),
            4,
            (&n as *const i64).cast(),
        ))
    }) else {
        return vec![];
    };
    let values = [num.0];
    let Some(a) = (unsafe {
        Owned::from_create(CFArrayCreate(
            std::ptr::null(),
            values.as_ptr(),
            1,
            kCFTypeArrayCallBacks.as_ptr().cast(),
        ))
    }) else {
        return vec![];
    };
    unsafe { Owned::from_create(crate::private_api::window_spaces(a.0)) }
        .map(|value| array(&value).map(number).collect())
        .unwrap_or_default()
}
fn geometry(el: Ref) -> (f64, f64, f64, f64) {
    let mut p = NSPoint::new(0.0, 0.0);
    let mut s = NSSize::new(0.0, 0.0);
    if let Some(v) = attr(el, ns_string!("AXPosition")) {
        unsafe {
            if CFGetTypeID(v.0) != AXValueGetTypeID()
                || !AXValueGetValue(v.0, 1, (&mut p as *mut NSPoint).cast())
            {
                crate::ipc_budget::record(-25201, false);
            }
        }
    } else {
        crate::ipc_budget::record(-25212, false);
    }
    if let Some(v) = attr(el, ns_string!("AXSize")) {
        unsafe {
            if CFGetTypeID(v.0) != AXValueGetTypeID()
                || !AXValueGetValue(v.0, 2, (&mut s as *mut NSSize).cast())
            {
                crate::ipc_budget::record(-25201, false);
            }
        }
    } else {
        crate::ipc_budget::record(-25212, false);
    }
    if ![p.x, p.y, s.width, s.height].iter().all(|v| v.is_finite()) {
        crate::ipc_budget::record(-25201, false);
    }
    (p.x, p.y, s.width, s.height)
}

fn native_tab_group(el: Ref) -> Option<Owned> {
    let children = attr(el, ns_string!("AXChildren"))?;
    // Native AppKit tab bars are direct window children. Do not descend into
    // web content or terminal panes, or retain their accessibility objects.
    for child in array(&children).take(16) {
        if text_attr(child, ns_string!("AXRole")) == "AXTabGroup" {
            // SAFETY: retain this borrowed array element before releasing its array.
            return unsafe { Owned::from_borrowed(child) };
        }
        if ipc_budget::expired() {
            break;
        }
    }
    None
}
fn native_tab_info(
    el: Ref,
    previous: Option<&Window>,
    cursor: usize,
    cache: &mut HashMap<u64, TabElement>,
) -> Option<(Vec<crate::models::WindowTab>, usize, usize)> {
    // Finish names from the retained controls on the next slice. Repeatedly
    // traversing the tab bar can otherwise spend the entire short budget.
    let pending = previous.filter(|w| {
        w.tabs.iter().any(|t| !t.resolved) && w.tabs.iter().all(|t| cache.contains_key(&t.id))
    });
    let mut source = None;
    let (raw, count) = if let Some(w) = pending {
        (
            w.tabs
                .iter()
                .map(|t| cache[&t.id].element.0)
                .collect::<Vec<_>>(),
            w.tab_count,
        )
    } else {
        let Some(group) = native_tab_group(el) else {
            return ipc_budget::healthy().then_some((Vec::new(), 0, 0));
        };
        let tabs = attr(group.0, ns_string!("AXTabs"))?;
        let raw: Vec<_> = array(&tabs).collect();
        let count = raw.len();
        source = Some(tabs);
        (raw, count)
    };
    let limit = count.min(128);
    let mut result = previous
        .filter(|w| w.tab_count == count)
        .map(|w| w.tabs.clone())
        .unwrap_or_default();
    for (index, tab) in raw.iter().copied().enumerate().take(limit) {
        // SAFETY: source or the cache owns every AX element during comparison.
        let known = result
            .get(index)
            .map(|t| t.id)
            .filter(|id| {
                cache
                    .get(id)
                    .is_some_and(|old| unsafe { CFEqual(old.element.0, tab) })
            })
            .or_else(|| {
                cache
                    .iter()
                    .find(|(_, old)| unsafe { CFEqual(old.element.0, tab) })
                    .map(|(id, _)| *id)
            });
        let id = if let Some(id) = known {
            let cached = cache.get_mut(&id).unwrap();
            cached.seen = Instant::now();
            // SAFETY: discovery retains both the current and cached owners.
            if !unsafe { CFEqual(cached.owner.0, el) } {
                cached.owner = unsafe { Owned::from_borrowed(el) }?;
            }
            id
        } else {
            if cache.len() >= 4096 {
                break;
            }
            // SAFETY: source or the cache still owns this borrowed element.
            let Some(element) = (unsafe { Owned::from_borrowed(tab) }) else {
                break;
            };
            let id = NEXT_TAB_ID.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
            cache.insert(
                id,
                TabElement {
                    element,
                    // SAFETY: the owning window stays alive during discovery.
                    owner: unsafe { Owned::from_borrowed(el) }?,
                    seen: Instant::now(),
                },
            );
            id
        };
        if index >= result.len() {
            result.push(crate::models::WindowTab {
                id,
                ..Default::default()
            });
        } else if result[index].id != id {
            result[index] = crate::models::WindowTab {
                id,
                ..Default::default()
            };
        }
    }
    result.truncate(limit);
    let start = result
        .iter()
        .position(|t| !t.resolved)
        .unwrap_or(cursor.min(result.len().saturating_sub(1)));
    let mut next = start;
    for (index, tab) in raw.into_iter().enumerate().take(result.len()).skip(start) {
        if ipc_budget::expired() {
            break;
        }
        let title = text_attr(tab, ns_string!("AXTitle"))
            .chars()
            .take(256)
            .collect();
        if !ipc_budget::healthy() {
            break;
        }
        // AppKit can return AXErrorFailure for an inactive tab's AXValue.
        // Its identity and name remain valid. This optional query must not
        // discard them or delay discovery of the next tab.
        let (selected, _) = ipc_budget::run(ipc_budget::remaining(), || {
            // SAFETY: source or the cache retains this tab throughout the query.
            attr(tab, ns_string!("AXValue")).is_some_and(|v| unsafe {
                (CFGetTypeID(v.0) == CFBooleanGetTypeID() && CFBooleanGetValue(v.0))
                    || (CFGetTypeID(v.0) == CFNumberGetTypeID() && number(v.0) != 0)
            })
        });
        result[index].title = title;
        result[index].selected = selected;
        result[index].resolved = true;
        next = if index + 1 == limit { 0 } else { index + 1 };
    }
    drop(source);
    Some((result, count, next))
}
fn parent_window(el: Ref) -> Option<u32> {
    let parent = attr(el, ns_string!("AXParent"))?;
    if text_attr(parent.0, ns_string!("AXRole")) != "AXWindow" {
        return None;
    }
    // SAFETY: the retained parent AX element stays alive on this worker.
    unsafe { crate::private_api::window_id(parent.0) }
}

mod service;
pub fn worker(rx: Receiver<Command>, tx: std::sync::mpsc::SyncSender<Snapshot>) {
    service::worker(rx, tx)
}
fn scan(elements: &mut HashMap<u32, Owned>) -> Snapshot {
    service::snapshot(elements, Duration::from_secs(2))
}
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
enum DeferredClose {
    Window(u32),
    Tab(u64),
}
fn tab_pid(target: &TabElement) -> Option<i32> {
    unsafe extern "C" {
        fn AXUIElementGetPid(el: Ref, pid: *mut i32) -> i32;
    }
    let mut pid = 0;
    // SAFETY: the worker retains the owner during this native query.
    (unsafe { AXUIElementGetPid(target.owner.0, &mut pid) } == 0).then_some(pid)
}
fn native_tab_window(target: &TabElement, require_selected: bool) -> Option<Owned> {
    let pid = tab_pid(target)?;
    // SAFETY: this worker owns the Create result and releases it after use.
    let root = unsafe { Owned::from_create(AXUIElementCreateApplication(pid)) }?;
    let focused = attr(root.0, ns_string!("AXFocusedWindow"))?;
    let group = native_tab_group(focused.0)?;
    let tabs = attr(group.0, ns_string!("AXTabs"))?;
    let belongs = array(&tabs).take(128).any(|tab| {
        // SAFETY: the copied array and target cache retain both AX elements.
        unsafe { CFEqual(tab, target.element.0) }
    });
    // AppKit only exposes a true AXValue on its selected radio tab. A missing
    // or late value must never authorize closing some other focused window.
    (belongs && (!require_selected || bool_attr(target.element.0, ns_string!("AXValue"))))
        .then_some(focused)
}
fn focused_tab_window(target: &TabElement) -> Option<Owned> {
    native_tab_window(target, true)
}
// Deferred controls wait for native restore or tab selection before one close press.
fn execute(
    command: Command,
    elements: &HashMap<u32, Owned>,
    tabs: &HashMap<u64, TabElement>,
) -> Result<Option<DeferredClose>, String> {
    if crate::runtime::stopping() {
        return Err("Window service is stopping".into());
    }
    let window = |id| {
        elements.get(&id).ok_or_else(|| {
            "Window is no longer available; wait for discovery to refresh".to_string()
        })
    };
    let check = |ok: bool| {
        if ok {
            Ok(())
        } else {
            Err("The application rejected or timed out during window control".into())
        }
    };
    let result = match command {
        Command::Launch(bundle, mode) => launch(&bundle, &mode),
        Command::Dock(hidden) => crate::dock::apply(hidden),
        Command::SelectTab(id) | Command::CloseTab(id) | Command::CloseSelectedTab(id) => {
            let target = tabs
                .get(&id)
                .ok_or("Native tab changed; wait for discovery to refresh")?;
            if matches!(command, Command::CloseSelectedTab(..)) {
                if objc2_app_kit::NSWorkspace::sharedWorkspace()
                    .frontmostApplication()
                    .map(|app| app.processIdentifier())
                    != tab_pid(target)
                {
                    return Ok(Some(DeferredClose::Tab(id)));
                }
                let Some(focused) = focused_tab_window(target) else {
                    return Ok(Some(DeferredClose::Tab(id)));
                };
                if !ipc_budget::healthy() {
                    return Ok(Some(DeferredClose::Tab(id)));
                }
                let button = attr(focused.0, ns_string!("AXCloseButton"));
                if !ipc_budget::healthy() || button.is_none() {
                    return Ok(Some(DeferredClose::Tab(id)));
                }
                check(action(button.unwrap().0, ns_string!("AXPress")))
            } else {
                let pid = tab_pid(target).ok_or("Cannot identify native tab owner")?;
                check(activate_pid(pid))?;
                if bool_attr(target.owner.0, ns_string!("AXMinimized")) {
                    check(set_bool(target.owner.0, ns_string!("AXMinimized"), false))?;
                }
                let same_group = ipc_budget::run(ipc_budget::remaining(), || {
                    native_tab_window(target, false).is_some()
                })
                .0;
                if !same_group {
                    set_bool_tracking(target.owner.0, ns_string!("AXMain"), true, false);
                    check(action(target.owner.0, ns_string!("AXRaise")))
                        .map_err(|e| format!("{e}: native owner raise"))?;
                }
                // Native tab selection is idempotent. Raising the old owner
                // after this press would switch back to the previous tab.
                let selected = action(target.element.0, ns_string!("AXPress"));
                if matches!(command, Command::CloseTab(..)) {
                    // Poll exact native identity before closing. Never retry
                    // an actual close press, including after a late AX reply.
                    return Ok(Some(DeferredClose::Tab(id)));
                }
                check(selected).map_err(|e| format!("{e}: native tab selection"))
            }
        }
        Command::Activate(id, hide) => {
            let w = window(id)?;
            if hide {
                check(set_bool(w.0, ns_string!("AXMinimized"), true))
            } else {
                let mut pid = 0;
                unsafe extern "C" {
                    fn AXUIElementGetPid(el: Ref, pid: *mut i32) -> i32;
                }
                let error = unsafe { AXUIElementGetPid(w.0, &mut pid) };
                if error != 0 {
                    return Err(format!("Cannot identify window owner: AX {error}"));
                }
                // Start the app's unhide/focus transition before AX restore or
                // raise. A late AX reply must not prevent app activation.
                check(activate_pid(pid))?;
                let minimized = bool_attr(w.0, ns_string!("AXMinimized"));
                if !crate::ipc_budget::healthy() {
                    return Err("Cannot read window state".into());
                }
                if minimized {
                    check(set_bool(w.0, ns_string!("AXMinimized"), false))?;
                }
                // Utility panels cannot become main windows. Asking them to do
                // so can redirect activation to their owner's main window.
                if text_attr(w.0, ns_string!("AXSubrole")) == "AXStandardWindow" {
                    set_bool_tracking(w.0, ns_string!("AXMain"), true, false);
                }
                check(action(w.0, ns_string!("AXRaise")))?;
                Ok(())
            }
        }
        Command::Minimize(id) => {
            let w = window(id)?;
            let value = bool_attr(w.0, ns_string!("AXMinimized"));
            if !crate::ipc_budget::healthy() {
                return Err("Cannot read window state".into());
            }
            check(set_bool(w.0, ns_string!("AXMinimized"), !value))
        }
        Command::Fullscreen(id) => {
            let w = window(id)?;
            let value = bool_attr(w.0, ns_string!("AXFullScreen"));
            if !crate::ipc_budget::healthy() {
                return Err("Cannot read fullscreen state".into());
            }
            check(set_bool(w.0, ns_string!("AXFullScreen"), !value))
        }
        Command::Close(id) | Command::CloseRestored(id) => {
            let w = window(id)?;
            let button = attr(w.0, ns_string!("AXCloseButton"));
            let unavailable = button.as_ref().is_none_or(|button| {
                attr(button.0, ns_string!("AXEnabled")).is_some_and(|value| unsafe {
                    CFGetTypeID(value.0) == CFBooleanGetTypeID() && !CFBooleanGetValue(value.0)
                })
            });
            if !crate::ipc_budget::healthy() {
                // Restore animation can temporarily reject read-only AX queries.
                // The worker bounds this wait and never repeats AXPress.
                if matches!(command, Command::CloseRestored(_)) {
                    return Ok(Some(DeferredClose::Window(id)));
                }
                return Err("Cannot read window close controls".into());
            }
            if unavailable {
                if matches!(command, Command::CloseRestored(_)) {
                    return Ok(Some(DeferredClose::Window(id)));
                }
                let minimized = bool_attr(w.0, ns_string!("AXMinimized"));
                if !crate::ipc_budget::healthy() {
                    return Err("Cannot read minimized window state".into());
                }
                if minimized {
                    check(set_bool(w.0, ns_string!("AXMinimized"), false))?;
                    return Ok(Some(DeferredClose::Window(id)));
                }
            }
            let button = button.ok_or("Window has no accessible close button")?;
            check(action(button.0, ns_string!("AXPress")))
        }
        Command::Hide(pid) | Command::Quit(pid) => {
            let app = NSRunningApplication::runningApplicationWithProcessIdentifier(pid)
                .ok_or("Application is no longer running")?;
            check(match command {
                Command::Hide(_) => app.hide(),
                _ => app.terminate(),
            })
        }
        Command::Resize(id, max_y) => {
            let w = window(id)?;
            let (_, y, width, height) = geometry(w.0);
            if !crate::ipc_budget::healthy() {
                return Err("Cannot read window geometry".into());
            }
            if !bool_attr(w.0, ns_string!("AXFullScreen"))
                && y + height > max_y
                && max_y - y > 100.0
            {
                let size = NSSize::new(width, max_y - y);
                let value = unsafe {
                    Owned::from_create(AXValueCreate(2, (&size as *const NSSize).cast()))
                }
                .ok_or("Cannot create window size")?;
                let timeout = crate::ipc_budget::timeout().ok_or("Window resize budget expired")?;
                unsafe {
                    AXUIElementSetMessagingTimeout(w.0, timeout);
                    check(
                        AXUIElementSetAttributeValue(w.0, cf_string(ns_string!("AXSize")), value.0)
                            == 0,
                    )
                }
            } else {
                Ok(())
            }
        }
        Command::Probe(_) | Command::Stop => Ok(()),
    };
    result.map(|()| None)
}
fn activate_pid(pid: i32) -> bool {
    NSRunningApplication::runningApplicationWithProcessIdentifier(pid).is_some_and(|app| {
        // An already active app needs only its window or native tab raised.
        // macOS can reject a redundant app-activation request during tab changes.
        if app.isActive() {
            return true;
        }
        let source = NSRunningApplication::currentApplication();
        let options = NSApplicationActivationOptions::empty();
        // Only an active source can yield focus. Taskbar panels normally keep
        // the other app active, so use an ordinary request in that case.
        if source.isActive() {
            app.activateFromApplication_options(&source, options)
        } else {
            app.activateWithOptions(options)
        }
    })
}

pub fn installed_apps() -> Vec<Application> {
    let mut output = vec![];
    let mut seen = HashSet::new();
    for root in [
        "/Applications",
        "/System/Applications",
        "/System/Applications/Utilities",
        "/System/Library/CoreServices/Applications",
    ] {
        walk_apps(std::path::Path::new(root), 0, &mut output, &mut seen);
    }
    let finder = std::path::Path::new("/System/Library/CoreServices/Finder.app");
    if finder.exists() && seen.insert("com.apple.finder".into()) {
        output.push(Application {
            bundle: "com.apple.finder".into(),
            name: "Finder".into(),
            path: finder.to_string_lossy().into(),
        });
    }
    if let Some(home) = std::env::var_os("HOME") {
        walk_apps(
            &std::path::PathBuf::from(home).join("Applications"),
            0,
            &mut output,
            &mut seen,
        );
    }
    sort_applications(&mut output);
    output
}
fn sort_applications(apps: &mut [Application]) {
    apps.sort_by_cached_key(|a| a.name.to_lowercase());
}
fn walk_apps(
    root: &std::path::Path,
    depth: usize,
    out: &mut Vec<Application>,
    seen: &mut HashSet<String>,
) {
    let Ok(entries) = std::fs::read_dir(root) else {
        return;
    };
    for entry in entries.flatten() {
        let p = entry.path();
        if p.extension().is_some_and(|s| s == "app") {
            if let Some(bundle) = objc2_foundation::NSBundle::bundleWithPath(&NSString::from_str(
                &p.to_string_lossy(),
            ))
            .and_then(|b| b.bundleIdentifier())
            {
                let bundle = bundle.to_string();
                if seen.insert(bundle.clone()) {
                    out.push(Application {
                        bundle,
                        name: p.file_stem().unwrap().to_string_lossy().into(),
                        path: p.to_string_lossy().into(),
                    });
                }
            }
        } else if depth < 1 && p.is_dir() {
            walk_apps(&p, depth + 1, out, seen);
        }
    }
}
pub fn open_url(url: &str) {
    if let Some(url) = NSURL::URLWithString(&NSString::from_str(url)) {
        NSWorkspace::sharedWorkspace().openURL(&url);
    }
}
pub fn launch(bundle: &str, action: &str) -> Result<(), String> {
    let workspace = NSWorkspace::sharedWorkspace();
    let url = workspace.URLForApplicationWithBundleIdentifier(&NSString::from_str(bundle));
    let Some(url) = url else {
        return Err("Application is no longer installed".into());
    };
    if action != "launchOrActivateApp" {
        let private = action == "googleChromeNewIncognitoWindow"
            || action == "safariNewPrivateWindow"
            || action == "firefoxNewPrivateWindow";
        let key = if private && bundle == "org.mozilla.firefox" {
            "p"
        } else {
            "n"
        };
        let shift =
            private || bundle == "com.microsoft.VSCode" || bundle == "com.microsoft.VSCodeInsiders";
        if new_window(bundle, key, if shift { 1 } else { 0 }) {
            return Ok(());
        }
        if private {
            let running = NSRunningApplication::runningApplicationsWithBundleIdentifier(
                &NSString::from_str(bundle),
            );
            if !running.is_empty() {
                // Do not silently turn a failed private action into a normal window.
                return Err("Could not open the requested private window".into());
            }
            if let Some(argument) = match bundle {
                "com.google.Chrome" => Some("--incognito"),
                "org.mozilla.firefox" => Some("--private-window"),
                _ => None,
            } {
                std::process::Command::new("/usr/bin/open")
                    .args(["-b", bundle, "--args", argument])
                    .spawn()
                    .map_err(|e| e.to_string())?;
                return Ok(());
            }
            workspace.openURL(&url);
            // A cold Safari launch must expose its menu before AX can invoke
            // New Private Window. This wait belongs to the separate launch worker.
            for _ in 0..10 {
                std::thread::sleep(Duration::from_millis(250));
                if new_window(bundle, key, 1) {
                    return Ok(());
                }
            }
            return Err("Safari did not expose its private-window command".into());
        }
    }
    let options = 0usize;
    unsafe {
        let opened: Option<Retained<AnyObject>> = msg_send![&*workspace,openURL:&*url, options:options, configuration: &*objc2_foundation::NSDictionary::<NSString,AnyObject>::new(), error:std::ptr::null_mut::<*mut AnyObject>()];
        opened
            .map(|_| ())
            .ok_or_else(|| "Application launch failed".into())
    }
}
fn new_window(bundle: &str, key: &str, modifiers: i64) -> bool {
    if !trusted() {
        return false;
    }
    let running =
        NSRunningApplication::runningApplicationsWithBundleIdentifier(&NSString::from_str(bundle));
    let Some(app) = running.firstObject() else {
        return false;
    };
    let pid = app.processIdentifier();
    activate_pid(pid);
    let Some(ax) = (unsafe { Owned::from_create(AXUIElementCreateApplication(pid)) }) else {
        return false;
    };
    unsafe {
        AXUIElementSetMessagingTimeout(ax.0, 0.12);
    }
    let Some(menu) = attr(ax.0, ns_string!("AXMenuBar")) else {
        return false;
    };
    fn visit(
        el: Ref,
        depth: usize,
        budget: &mut usize,
        started: Instant,
        key: &str,
        modifiers: i64,
    ) -> bool {
        if depth > 4 || *budget == 0 || started.elapsed() > Duration::from_millis(350) {
            return false;
        }
        *budget -= 1;
        if text_attr(el, ns_string!("AXMenuItemCmdChar")).eq_ignore_ascii_case(key)
            && attr(el, ns_string!("AXMenuItemCmdModifiers"))
                .is_some_and(|v| number(v.0) == modifiers)
            && bool_attr(el, ns_string!("AXEnabled"))
        {
            return action(el, ns_string!("AXPress"));
        }
        if let Some(children) = attr(el, ns_string!("AXChildren")) {
            for c in array(&children) {
                if visit(c, depth + 1, budget, started, key, modifiers) {
                    return true;
                }
            }
        }
        false
    }
    visit(menu.0, 0, &mut 160, Instant::now(), key, modifiers)
}

pub fn login(enabled: bool) -> Result<(), String> {
    let cls = AnyClass::get(c"SMAppService").ok_or("ServiceManagement unavailable")?;
    let service: Retained<AnyObject> = unsafe { msg_send![cls, mainAppService] };
    let status: isize = unsafe { msg_send![&*service, status] };
    if (enabled && status == 1) || (!enabled && status == 0) {
        return Ok(());
    }
    let mut error: *mut AnyObject = std::ptr::null_mut();
    let ok: bool = unsafe {
        if enabled {
            msg_send![&*service,registerAndReturnError:&mut error]
        } else {
            msg_send![&*service,unregisterAndReturnError:&mut error]
        }
    };
    if ok {
        Ok(())
    } else if !error.is_null() {
        let desc: Retained<NSString> = unsafe { msg_send![error, localizedDescription] };
        Err(desc.to_string())
    } else {
        Err("Login registration failed".into())
    }
}
pub fn login_status() -> isize {
    AnyClass::get(c"SMAppService")
        .map(|cls| unsafe {
            let service: Retained<AnyObject> = msg_send![cls, mainAppService];
            msg_send![&*service, status]
        })
        .unwrap_or(-1)
}

// Screenshot results carry one retained CGImage across the callback boundary.
pub(crate) struct Image(pub(crate) Ref);
impl Clone for Image {
    fn clone(&self) -> Self {
        Self(unsafe { CFRetain(self.0) })
    }
}
// CGImage is immutable and CF retain/release is thread-safe. One owner crosses the channel.
unsafe impl Send for Image {}
impl Drop for Image {
    fn drop(&mut self) {
        unsafe { CFRelease(self.0) }
    }
}
pub fn screenshot(
    id: u32,
    width: isize,
    height: isize,
    tx: Sender<Result<(u32, Image), String>>,
    mode: crate::config::CaptureMode,
) {
    crate::capture::screenshot(id, width, height, tx, mode);
}

pub fn import_pins(config: &mut Config) {
    if Config::path().exists() {
        return;
    }
    let defaults = objc2_foundation::NSUserDefaults::standardUserDefaults();
    if let Some(domain) =
        defaults.persistentDomainForName(&NSString::from_str("com.fpfxtknjju.wbgcdolfev"))
        && let Some(v) = domain.objectForKey(&NSString::from_str("pinList"))
        && let Some(s) = v.downcast_ref::<NSString>()
        && let Ok(pins) = serde_json::from_str::<Vec<crate::config::Pin>>(&s.to_string())
    {
        config.pins = pins;
    }
}
/// Read the user's Dock application sequence without changing Dock preferences.
pub fn dock_app_order() -> Vec<String> {
    use objc2_foundation::NSDictionary;
    let defaults = objc2_foundation::NSUserDefaults::standardUserDefaults();
    let Some(domain) = defaults.persistentDomainForName(&NSString::from_str("com.apple.dock"))
    else {
        return vec![];
    };
    let Some(value) = domain.objectForKey(&NSString::from_str("persistent-apps")) else {
        return vec![];
    };
    let Some(tiles) = value.downcast_ref::<NSArray<AnyObject>>() else {
        return vec![];
    };
    let mut result = Vec::new();
    for tile in tiles.iter().take(crate::window_order::MAX_APPS) {
        let Some(tile) = tile.downcast_ref::<NSDictionary>() else {
            continue;
        };
        let Some(data) = tile.objectForKey(&NSString::from_str("tile-data")) else {
            continue;
        };
        let Some(data) = data.downcast_ref::<NSDictionary>() else {
            continue;
        };
        if let Some(bundle) = data.objectForKey(&NSString::from_str("bundle-identifier"))
            && let Some(bundle) = bundle.downcast_ref::<NSString>()
        {
            let bundle = bundle.to_string();
            if !bundle.is_empty() && !result.contains(&bundle) {
                result.push(bundle);
            }
        }
    }
    result
}
pub fn benchmark(count: usize, lifecycle: bool) -> bool {
    // Read-only capture workload; emitted metrics contain no window titles or image data.
    let snapshot = autoreleasepool(|_| scan(&mut HashMap::new()));
    if !snapshot.trusted || !snapshot.screen_allowed {
        println!(
            "{}",
            serde_json::json!({"error":"Accessibility and Screen Recording required"})
        );
        return false;
    }
    let Some(w) = snapshot
        .windows
        .iter()
        .find(|w| w.bundle == "com.apple.TextEdit" && !w.minimized)
        .or_else(|| {
            snapshot
                .windows
                .iter()
                .find(|w| w.on_space && !w.minimized && !w.fullscreen)
        })
    else {
        println!("{{\"error\":\"No preview window\"}}");
        return false;
    };
    let id = w.id;
    let start = Instant::now();
    let mut successes = 0;
    let mut errors = vec![];
    let mut elapsed = vec![];
    let count = count.min(10000);
    for i in 0..count {
        let (tx, rx) = std::sync::mpsc::channel();
        let t = Instant::now();
        // Alternate thumbnail dimensions to exercise source replacement, and
        // pause every fourth request to exercise idle stop and same-source restart.
        let width = if lifecycle && (i / 8) % 2 == 1 {
            480
        } else {
            560
        };
        autoreleasepool(|_| {
            screenshot(
                id,
                width,
                (width as f64 * w.height / w.width.max(1.0)) as isize,
                tx,
                crate::config::CaptureMode::Stream,
            )
        });
        match rx.recv_timeout(Duration::from_secs(15)) {
            Ok(Ok((_id, image))) => {
                drop(image);
                successes += 1;
                elapsed.push(t.elapsed().as_millis() as u64);
            }
            Ok(Err(e)) => {
                if errors.len() < 5 {
                    errors.push(e);
                }
            }
            Err(e) => {
                errors.push(e.to_string());
                break;
            }
        }
        std::thread::sleep(Duration::from_millis(if lifecycle && i % 4 == 3 {
            2500
        } else {
            100
        }));
    }
    elapsed.sort();
    crate::capture::shutdown();
    let (frames, sessions, stream_live, running) = crate::capture::counts();
    let (live, peak) = cf_counts();
    println!(
        "{}",
        serde_json::json!({"requested":count,"successes":successes,"errors":errors,"lifecycle":lifecycle,"elapsed_seconds":start.elapsed().as_secs_f64(),"median_ms":elapsed.get(elapsed.len()/2),"p95_ms":elapsed.get(elapsed.len()*95/100),"cf_live":live,"cf_peak":peak,"stream_frames":frames,"stream_sessions":sessions,"stream_live":stream_live,"stream_running":running,"capture_stop_errors":crate::capture::stop_errors(),"capture_native_state_uncertain":crate::capture::native_uncertain()})
    );
    successes == count
        && errors.is_empty()
        && stream_live == 0
        && crate::capture::stop_errors() == 0
        && !crate::capture::native_uncertain()
}

pub fn benchmark_scan(count: usize) -> bool {
    let count = count.min(10000);
    let start = Instant::now();
    let mut elements = HashMap::new();
    let mut scans = 0;
    let mut max_windows = 0;
    for _ in 0..count {
        let snapshot = autoreleasepool(|_| scan(&mut elements));
        if !snapshot.trusted {
            break;
        }
        max_windows = max_windows.max(snapshot.windows.len());
        scans += 1;
        std::thread::sleep(Duration::from_millis(20));
    }
    drop(elements);
    let (live, peak) = cf_counts();
    println!(
        "{}",
        serde_json::json!({"requested_scans":count,"scans":scans,"max_windows":max_windows,"elapsed_seconds":start.elapsed().as_secs_f64(),"cf_live":live,"cf_peak":peak})
    );
    scans == count && live == 0
}

/// Read-only snapshot for Rust integrations. Permission prompting belongs to the host.
pub(crate) fn window_snapshot() -> Snapshot {
    autoreleasepool(|_| scan(&mut HashMap::new()))
}

pub(crate) fn benchmark_discovery(seconds: u64) -> bool {
    service::benchmark(seconds)
}

pub(crate) fn benchmark_latency(seconds: u64, fixture: Option<i32>) -> bool {
    service::latency(seconds, fixture)
}

/// Activate only windows owned by the disposable native QA fixture.
pub(crate) fn check_fixture_activation(pid: i32) -> bool {
    // Match the app's passive AppKit context. A command-line process does not
    // otherwise initialize NSApplication or process activation-state updates.
    let app = objc2_app_kit::NSApplication::sharedApplication(
        objc2::MainThreadMarker::new().expect("main thread"),
    );
    app.setActivationPolicy(objc2_app_kit::NSApplicationActivationPolicy::Accessory);
    service::check_fixture_activation(pid)
}

pub(crate) fn check_fixture_related(pid: i32) -> bool {
    let app = objc2_app_kit::NSApplication::sharedApplication(
        objc2::MainThreadMarker::new().expect("main thread"),
    );
    app.setActivationPolicy(objc2_app_kit::NSApplicationActivationPolicy::Accessory);
    service::check_fixture_related(pid)
}

/// Close only a minimized window from the named disposable QA application.
pub(crate) fn check_fixture_close(pid: i32) -> bool {
    autoreleasepool(|_| {
        let mut elements = HashMap::new();
        let before = scan(&mut elements);
        let fixture =
            |w: &&Window| w.pid == pid && w.bundle == "io.sharif.taskbarrust.interactionfixture";
        let ids: HashSet<_> = before
            .windows
            .iter()
            .filter(fixture)
            .map(|w| w.id)
            .collect();
        let Some(target) = before.windows.iter().filter(fixture).find(|w| w.minimized) else {
            println!("{{\"passed\":false,\"error\":\"No minimized QA window\"}}");
            return false;
        };
        if !before.trusted || ids.len() < 2 {
            println!(
                "{{\"passed\":false,\"error\":\"Two QA windows and Accessibility access are required\"}}"
            );
            return false;
        }
        let target = target.id;
        let requests = if elements
            .get(&target)
            .is_some_and(|window| attr(window.0, ns_string!("AXCloseButton")).is_none())
        {
            3
        } else {
            1
        };
        drop(elements);
        let (tx, rx) = std::sync::mpsc::sync_channel(32);
        let (snap_tx, snap_rx) = std::sync::mpsc::sync_channel(2);
        let worker = std::thread::spawn(move || service::worker(rx, snap_tx));
        let started = Instant::now();
        let mut remaining = ids.clone();
        let mut submitted = false;
        let mut probes = Vec::new();
        let mut probe_failures = 0;
        let mut errors = Vec::new();
        while started.elapsed() < Duration::from_secs(5) {
            if let Ok(snapshot) = snap_rx.recv_timeout(Duration::from_millis(50)) {
                remaining = snapshot
                    .windows
                    .iter()
                    .filter(fixture)
                    .map(|w| w.id)
                    .collect();
                if let Some(error) = snapshot.control_error {
                    errors.push(error);
                }
                if !submitted && remaining == ids {
                    // Duplicate requests must not restore or press Close twice.
                    for _ in 0..requests {
                        let _ = tx.send(Command::Close(target));
                    }
                    submitted = true;
                }
                if submitted && !remaining.contains(&target) {
                    break;
                }
            }
            if submitted {
                let (reply, answer) = std::sync::mpsc::channel();
                let at = Instant::now();
                if tx.try_send(Command::Probe(reply)).is_ok()
                    && let Ok(reply) = answer.recv_timeout(Duration::from_millis(250))
                {
                    probes.push(reply.duration_since(at).as_secs_f64() * 1000.0);
                } else {
                    probe_failures += 1;
                }
            }
        }
        let _ = tx.send(Command::Stop);
        drop(tx);
        let _ = worker.join();
        let mut expected = ids;
        expected.remove(&target);
        let max_probe = probes.iter().copied().fold(0.0_f64, f64::max);
        let passed = submitted
            && remaining == expected
            && errors.is_empty()
            && probe_failures == 0
            && max_probe < 200.0;
        println!(
            "{}",
            serde_json::json!({"passed":passed,"target_was_minimized":true,"closed_only_target":remaining == expected,"remaining_windows":remaining.len(),"close_requests":requests,"probe_samples":probes.len(),"probe_failures":probe_failures,"max_probe_ms":max_probe,"errors":errors})
        );
        passed
    })
}

#[cfg(test)]
mod native_key_tests {
    use super::*;
    use objc2_foundation::{NSDictionary, NSNumber, ns_string};

    #[test]
    fn native_arrays_keep_their_owner_and_enforce_the_scan_limit() {
        autoreleasepool(|_| {
            let number_value = NSNumber::new_i64(42);
            // SAFETY: retain these Foundation objects before their original owner drops.
            let wrong_type =
                unsafe { Owned::from_borrowed(std::ptr::from_ref(&*number_value).cast()) }.unwrap();
            assert_eq!(array(&wrong_type).len(), 0);

            let empty = NSArray::<NSNumber>::new();
            let empty_owner =
                unsafe { Owned::from_borrowed(std::ptr::from_ref(&*empty).cast()) }.unwrap();
            assert_eq!(array(&empty_owner).len(), 0);

            let values = vec![number_value; 4_100];
            let native = NSArray::from_retained_slice(&values);
            let owner =
                unsafe { Owned::from_borrowed(std::ptr::from_ref(&*native).cast()) }.unwrap();
            drop(native);
            drop(values);
            let mut elements = array(&owner);
            assert_eq!(elements.len(), 4_096);
            assert_eq!(elements.next().map(number), Some(42));
            assert_eq!(elements.next_back().map(number), Some(42));
            assert_eq!(elements.len(), 4_094);
            assert_eq!(elements.map(number).sum::<i64>(), 4_094 * 42);
        });
    }

    #[test]
    fn native_dictionary_keys_preserve_unicode_and_reject_invalid_inputs() {
        autoreleasepool(|_| {
            let value = NSNumber::new_i64(42);
            let dictionary = NSDictionary::from_slices(
                &[ns_string!("kCGWindowNumber"), ns_string!("東京 👩🏽‍💻")],
                &[&*value, &*value],
            );
            let raw = std::ptr::from_ref(&*dictionary).cast();
            assert_eq!(number(dict(raw, ns_string!("kCGWindowNumber"))), 42);
            assert_eq!(number(dict(raw, ns_string!("東京 👩🏽‍💻"))), 42);
            assert!(dict(raw, ns_string!("absent")).is_null());
            assert!(dict(std::ptr::null(), ns_string!("kCGWindowNumber")).is_null());
            assert!(
                dict(
                    std::ptr::from_ref(&*value).cast(),
                    ns_string!("kCGWindowNumber")
                )
                .is_null()
            );
        });
    }

    #[test]
    #[ignore = "explicit native dictionary lookup microbenchmark"]
    fn benchmark_native_keys() {
        autoreleasepool(|_| {
            let value = NSNumber::new_i64(42);
            let dictionary =
                NSDictionary::from_slices(&[ns_string!("kCGWindowNumber")], &[&*value]);
            let raw = std::ptr::from_ref(&*dictionary).cast();
            assert_eq!(number(dict(raw, ns_string!("kCGWindowNumber"))), 42);
            let mut cases = Vec::new();
            for lookups in [8, 32, 64, 512] {
                let mut samples = Vec::new();
                for _ in 0..200 {
                    let start = Instant::now();
                    for _ in 0..lookups {
                        std::hint::black_box(dict(raw, ns_string!("kCGWindowNumber")));
                    }
                    samples.push(start.elapsed().as_secs_f64() * 1_000_000.0);
                }
                samples.sort_by(f64::total_cmp);
                cases.push(serde_json::json!({"lookups":lookups,"median_us":samples[100],"p95_us":samples[190]}));
            }
            println!("{}", serde_json::json!({"native_keys":cases}));
        });
    }

    #[test]
    #[ignore = "explicit application catalog sort microbenchmark"]
    fn benchmark_catalog_sort() {
        let mut cases = Vec::new();
        for count in [64, 512, 2048] {
            let apps: Vec<_> = (0..count)
                .map(|i| Application {
                    bundle: format!("qa.{i}"),
                    name: format!(
                        "{} {}",
                        ["Éditor", "editor", "EDITOR", "東京"][i % 4],
                        (i * 71) % 37
                    ),
                    path: format!("/QA/{i}.app"),
                })
                .collect();
            let mut timings = Vec::new();
            for cached in [false, true] {
                let mut samples = Vec::new();
                for _ in 0..200 {
                    let mut input = apps.clone();
                    let start = Instant::now();
                    if cached {
                        sort_applications(&mut input);
                    } else {
                        input.sort_by_key(|a| a.name.to_lowercase());
                    }
                    samples.push(start.elapsed().as_secs_f64() * 1_000_000.0);
                    std::hint::black_box(input);
                }
                samples.sort_by(f64::total_cmp);
                timings.push(serde_json::json!({"cached":cached,"median_us":samples[100],"p95_us":samples[190]}));
            }
            let mut reference = apps.clone();
            reference.sort_by_key(|a| a.name.to_lowercase());
            let mut result = apps;
            sort_applications(&mut result);
            assert_eq!(result, reference);
            cases.push(serde_json::json!({"apps":count,"timings":timings}));
        }
        println!("{}", serde_json::json!({"catalog_sort":cases}));
    }
}
