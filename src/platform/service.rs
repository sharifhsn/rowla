//! A fair, incremental AX actor. Native objects and run-loop sources stay here.
use super::*;
use crate::{ipc_budget, scheduler::FairQueue};
use std::{
    collections::VecDeque,
    sync::{
        Arc,
        atomic::{AtomicBool, Ordering},
    },
};
const SLICE: Duration = Duration::from_millis(20);
const MAX_PER_APP: usize = 512;
fn window_notifications() -> [&'static NSString; 6] {
    [
        ns_string!("AXMoved"),
        ns_string!("AXResized"),
        ns_string!("AXTitleChanged"),
        ns_string!("AXWindowMiniaturized"),
        ns_string!("AXWindowDeminiaturized"),
        ns_string!("AXUIElementDestroyed"),
    ]
}
#[link(name = "CoreFoundation", kind = "framework")]
unsafe extern "C" {
    fn CFRunLoopGetCurrent() -> Ref;
    fn CFRunLoopAddSource(loop_: Ref, source: Ref, mode: Ref);
    fn CFRunLoopRemoveSource(loop_: Ref, source: Ref, mode: Ref);
    fn CFRunLoopRunInMode(mode: Ref, seconds: f64, once: bool) -> i32;
    static kCFRunLoopDefaultMode: Ref;
}
#[link(name = "ApplicationServices", kind = "framework")]
unsafe extern "C" {
    fn AXObserverCreate(
        pid: i32,
        callback: unsafe extern "C" fn(Ref, Ref, Ref, *mut c_void),
        out: *mut Ref,
    ) -> i32;
    fn AXObserverAddNotification(
        observer: Ref,
        element: Ref,
        notification: Ref,
        context: *mut c_void,
    ) -> i32;
    fn AXObserverGetRunLoopSource(observer: Ref) -> Ref;
    fn AXObserverRemoveNotification(observer: Ref, element: Ref, name: Ref) -> i32;
}
unsafe extern "C" fn changed(_: Ref, _: Ref, _: Ref, context: *mut c_void) {
    if !context.is_null() {
        unsafe { &*context.cast::<AtomicBool>() }.store(true, Ordering::Relaxed);
    }
}
struct Registration {
    element: Owned,
    next: usize,
    remove_next: usize,
}
struct Observer {
    value: Owned,
    loop_: Ref,
    wake: Arc<AtomicBool>,
    registered: HashMap<u32, Registration>,
}
impl Observer {
    fn new(pid: i32, root: Ref) -> Option<Self> {
        let mut value = std::ptr::null();
        if unsafe { AXObserverCreate(pid, changed, &mut value) } != 0 {
            return None;
        }
        let value = unsafe { Owned::from_create(value) }?;
        let loop_ = unsafe { CFRunLoopGetCurrent() };
        let mut this = Self {
            value,
            loop_,
            wake: Arc::new(AtomicBool::new(false)),
            registered: HashMap::new(),
        };
        let mut count = 0;
        for name in [
            ns_string!("AXWindowCreated"),
            ns_string!("AXFocusedWindowChanged"),
            ns_string!("AXApplicationHidden"),
            ns_string!("AXApplicationShown"),
        ] {
            count += this.add(root, name) as usize;
        }
        if count == 0 {
            return None;
        }
        unsafe {
            CFRunLoopAddSource(
                loop_,
                AXObserverGetRunLoopSource(this.value.0),
                kCFRunLoopDefaultMode,
            )
        };
        Some(this)
    }
    fn add(&mut self, el: Ref, name: &NSString) -> bool {
        let Some(timeout) = ipc_budget::timeout() else {
            return false;
        };
        unsafe {
            AXUIElementSetMessagingTimeout(el, timeout);
            AXObserverAddNotification(
                self.value.0,
                el,
                cf_string(name),
                Arc::as_ptr(&self.wake).cast_mut().cast(),
            ) == 0
        }
    }
    fn window(&mut self, id: u32, el: Ref) {
        if !self.registered.contains_key(&id) {
            if self.registered.len() >= MAX_PER_APP {
                return;
            }
            let Some(element) = (unsafe { Owned::from_borrowed(el) }) else {
                return;
            };
            self.registered.insert(
                id,
                Registration {
                    element,
                    next: 0,
                    remove_next: 0,
                },
            );
        }
        let names = window_notifications();
        let registration = self.registered.get_mut(&id).unwrap();
        if registration.remove_next > 0 {
            registration.next = 0;
            registration.remove_next = 0;
        }
        loop {
            let registration = &self.registered[&id];
            let next = registration.next;
            if next == names.len() || ipc_budget::expired() {
                return;
            }
            self.add(el, names[next]);
            self.registered.get_mut(&id).unwrap().next += 1;
        }
    }
}
impl Observer {
    fn prune(&mut self, seen: &HashSet<u32>) {
        self.registered.retain(|id, registration| {
            if seen.contains(id) {
                registration.remove_next = 0;
                return true;
            }
            let names = window_notifications();
            while registration.remove_next < names.len() {
                let Some(timeout) = ipc_budget::timeout() else {
                    return true;
                };
                unsafe {
                    AXUIElementSetMessagingTimeout(registration.element.0, timeout);
                    AXObserverRemoveNotification(
                        self.value.0,
                        registration.element.0,
                        cf_string(names[registration.remove_next]),
                    );
                }
                registration.remove_next += 1;
            }
            false
        });
    }
}
impl Drop for Observer {
    fn drop(&mut self) {
        unsafe {
            CFRunLoopRemoveSource(
                self.loop_,
                AXObserverGetRunLoopSource(self.value.0),
                kCFRunLoopDefaultMode,
            )
        };
    }
}
struct WorkspaceEvents {
    center: Retained<AnyObject>,
    tokens: Vec<Retained<AnyObject>>,
    wake: Arc<AtomicBool>,
}
impl WorkspaceEvents {
    fn new() -> Self {
        let workspace: Retained<AnyObject> =
            unsafe { msg_send![AnyClass::get(c"NSWorkspace").unwrap(), sharedWorkspace] };
        let center: Retained<AnyObject> = unsafe { msg_send![&*workspace, notificationCenter] };
        let wake = Arc::new(AtomicBool::new(false));
        let mut tokens = Vec::new();
        for name in [
            ns_string!("NSWorkspaceDidLaunchApplicationNotification"),
            ns_string!("NSWorkspaceDidTerminateApplicationNotification"),
            ns_string!("NSWorkspaceDidActivateApplicationNotification"),
            ns_string!("NSWorkspaceDidHideApplicationNotification"),
            ns_string!("NSWorkspaceDidUnhideApplicationNotification"),
            ns_string!("NSWorkspaceActiveSpaceDidChangeNotification"),
        ] {
            let signal = wake.clone();
            let block = block2::RcBlock::new(move |_: *mut AnyObject| {
                signal.store(true, Ordering::Relaxed);
            });
            let token: Retained<AnyObject> = unsafe {
                msg_send![&*center,addObserverForName:name, object:std::ptr::null::<AnyObject>(), queue:std::ptr::null::<AnyObject>(), usingBlock:&*block]
            };
            tokens.push(token);
        }
        Self {
            center,
            tokens,
            wake,
        }
    }
}
impl Drop for WorkspaceEvents {
    fn drop(&mut self) {
        for token in &self.tokens {
            unsafe {
                let _: () = msg_send![&*self.center,removeObserver:&**token];
            }
        }
    }
}
#[derive(Clone, PartialEq)]
struct Meta {
    pid: i32,
    bundle: String,
    name: String,
    path: String,
    hidden: bool,
    launched: f64,
}
struct App {
    meta: Meta,
    root: Owned,
    observer: Option<Observer>,
    pending: Option<VecDeque<Owned>>,
    seen: HashSet<u32>,
    windows: HashMap<u32, Window>,
    stale: bool,
    last: Instant,
    focus: Option<Owned>,
    coverage_complete: bool,
}
#[derive(Clone)]
struct PublicWindow {
    id: u32,
    pid: i32,
    title: String,
    bounds: (f64, f64, f64, f64),
}
fn double(p: Ref) -> f64 {
    let mut v = 0.0;
    if !p.is_null() && unsafe { CFGetTypeID(p) == CFNumberGetTypeID() } {
        unsafe {
            CFNumberGetValue(p, 6, (&mut v as *mut f64).cast());
        }
    }
    v
}
fn public_windows() -> Vec<PublicWindow> {
    let Some(data) = (unsafe { Owned::from_create(CGWindowListCopyWindowInfo(16, 0)) }) else {
        return vec![];
    };
    array(data.0)
        .into_iter()
        .filter(|d| number(dict(*d, ns_string!("kCGWindowLayer"))) == 0)
        .map(|d| {
            let b = dict(d, ns_string!("kCGWindowBounds"));
            PublicWindow {
                id: number(dict(d, ns_string!("kCGWindowNumber"))) as u32,
                pid: number(dict(d, ns_string!("kCGWindowOwnerPID"))) as i32,
                title: string(dict(d, ns_string!("kCGWindowName"))),
                bounds: (
                    double(dict(b, ns_string!("X"))),
                    double(dict(b, ns_string!("Y"))),
                    double(dict(b, ns_string!("Width"))),
                    double(dict(b, ns_string!("Height"))),
                ),
            }
        })
        .collect()
}
fn match_id(pid: i32, title: &str, b: (f64, f64, f64, f64), cg: &[PublicWindow]) -> Option<u32> {
    let mut candidates = cg.iter().filter(|w| {
        w.pid == pid
            && [
                (w.bounds.0, b.0),
                (w.bounds.1, b.1),
                (w.bounds.2, b.2),
                (w.bounds.3, b.3),
            ]
            .iter()
            .all(|(a, b)| (a - b).abs() < 2.0)
    });
    let first = candidates.next()?;
    if candidates.next().is_none() {
        return Some(first.id);
    }
    let mut exact = cg
        .iter()
        .filter(|w| w.pid == pid && !title.is_empty() && w.title == title && w.bounds == b);
    let first = exact.next()?;
    exact.next().is_none().then_some(first.id)
}
struct BadgeJob {
    pending: VecDeque<(Owned, usize)>,
    names: HashMap<String, String>,
    result: HashMap<String, String>,
    visited: usize,
}
impl BadgeJob {
    fn new() -> Option<Self> {
        let workspace: Retained<AnyObject> =
            unsafe { msg_send![AnyClass::get(c"NSWorkspace").unwrap(), sharedWorkspace] };
        let apps: Retained<NSArray<AnyObject>> =
            unsafe { msg_send![&*workspace, runningApplications] };
        let mut names = HashMap::new();
        let mut dock = 0;
        for app in &apps {
            let bundle: Option<Retained<NSString>> = unsafe { msg_send![&*app, bundleIdentifier] };
            let name: Option<Retained<NSString>> = unsafe { msg_send![&*app, localizedName] };
            if let Some(bundle) = bundle {
                let bundle = bundle.to_string();
                if bundle == "com.apple.dock" {
                    dock = unsafe { msg_send![&*app, processIdentifier] };
                }
                if let Some(name) = name {
                    names.insert(name.to_string(), bundle);
                }
            }
        }
        if dock == 0 {
            return None;
        }
        let root = unsafe { Owned::from_create(AXUIElementCreateApplication(dock)) }?;
        Some(Self {
            pending: VecDeque::from([(root, 0)]),
            names,
            result: HashMap::new(),
            visited: 0,
        })
    }
    fn slice(&mut self) {
        while !ipc_budget::expired() && self.visited < 200 {
            let Some((el, depth)) = self.pending.pop_front() else {
                return;
            };
            let badge = text_attr(el.0, ns_string!("AXStatusLabel"));
            let title = if badge.is_empty() {
                String::new()
            } else {
                text_attr(el.0, ns_string!("AXTitle"))
            };
            let children = if depth < 3 {
                attr(el.0, ns_string!("AXChildren"))
            } else {
                None
            };
            if !ipc_budget::healthy() {
                self.pending.push_front((el, depth));
                return;
            }
            self.visited += 1;
            if !badge.is_empty()
                && let Some(bundle) = self.names.get(&title)
            {
                self.result.insert(bundle.clone(), badge);
            }
            if let Some(children) = children {
                for child in array(children.0)
                    .into_iter()
                    .take(200 - self.pending.len().min(200))
                {
                    if let Some(child) = unsafe { Owned::from_borrowed(child) } {
                        self.pending.push_back((child, depth + 1));
                    }
                }
            }
        }
        if self.visited >= 200 {
            self.pending.clear();
        }
    }
}
struct Service {
    apps: HashMap<i32, App>,
    queue: FairQueue,
    elements: HashMap<u32, Owned>,
    visible: HashSet<u32>,
    spaces: HashSet<i64>,
    cg: Vec<PublicWindow>,
    front: i32,
    events: WorkspaceEvents,
    status: crate::models::DiscoveryStatus,
    badges: HashMap<String, String>,
    badge_job: Option<BadgeJob>,
    next_badge: Instant,
    next_meta: Instant,
    truncated: bool,
    control_error: Option<String>,
    fixture_pid: Option<i32>,
}
impl Service {
    fn new() -> Self {
        Self {
            apps: HashMap::new(),
            queue: FairQueue::default(),
            elements: HashMap::new(),
            visible: HashSet::new(),
            spaces: HashSet::new(),
            cg: vec![],
            front: 0,
            events: WorkspaceEvents::new(),
            status: Default::default(),
            badges: HashMap::new(),
            badge_job: None,
            next_badge: Instant::now(),
            next_meta: Instant::now(),
            truncated: false,
            control_error: None,
            fixture_pid: None,
        }
    }
    fn reconcile(&mut self) {
        self.next_meta = Instant::now() + Duration::from_secs(1);
        self.truncated = false;
        if !trusted() {
            self.apps.clear();
            self.elements.clear();
            self.queue.set_live([]);
            return;
        }
        self.cg = public_windows();
        self.spaces = active_spaces();
        self.visible.clear();
        if let Some(data) = unsafe { Owned::from_create(CGWindowListCopyWindowInfo(17, 0)) } {
            for d in array(data.0) {
                self.visible
                    .insert(number(dict(d, ns_string!("kCGWindowNumber"))) as u32);
            }
        }
        let workspace: Retained<AnyObject> =
            unsafe { msg_send![AnyClass::get(c"NSWorkspace").unwrap(), sharedWorkspace] };
        let running: Retained<NSArray<AnyObject>> =
            unsafe { msg_send![&*workspace, runningApplications] };
        let front: Option<Retained<AnyObject>> =
            unsafe { msg_send![&*workspace, frontmostApplication] };
        self.front = front
            .map(|a| unsafe { msg_send![&*a, processIdentifier] })
            .unwrap_or(0);
        let mut live = HashSet::new();
        for app in &running {
            let pid: i32 = unsafe { msg_send![&*app, processIdentifier] };
            // Native fixture checks need prompt focus reports from their owned
            // provider. Production discovery has no fixture filter.
            if self.fixture_pid.is_some_and(|fixture| pid != fixture) {
                continue;
            }
            let policy: isize = unsafe { msg_send![&*app, activationPolicy] };
            if (policy != 0 && Some(pid) != self.fixture_pid) || pid == std::process::id() as i32 {
                continue;
            }
            if live.len() >= 256 {
                self.truncated = true;
                break;
            }
            let bundle: Option<Retained<NSString>> = unsafe { msg_send![&*app, bundleIdentifier] };
            let bundle = bundle.map(|v| v.to_string()).unwrap_or_default();
            if bundle == "com.fpfxtknjju.wbgcdolfev" {
                continue;
            }
            let name: Option<Retained<NSString>> = unsafe { msg_send![&*app, localizedName] };
            let url: Option<Retained<NSURL>> = unsafe { msg_send![&*app, bundleURL] };
            let date: Option<Retained<AnyObject>> = unsafe { msg_send![&*app, launchDate] };
            let meta = Meta {
                pid,
                bundle,
                name: name.map(|v| v.to_string()).unwrap_or_default(),
                path: url
                    .and_then(|u| u.path())
                    .map(|v| v.to_string())
                    .unwrap_or_default(),
                hidden: unsafe { msg_send![&*app, isHidden] },
                launched: date
                    .map(|d| unsafe { msg_send![&*d, timeIntervalSince1970] })
                    .unwrap_or(0.0),
            };
            live.insert(pid);
            if self
                .apps
                .get(&pid)
                .is_some_and(|a| a.meta.launched != meta.launched)
            {
                self.apps.remove(&pid);
            }
            if let Some(a) = self.apps.get_mut(&pid) {
                if a.meta != meta {
                    a.meta = meta;
                    a.last = Instant::now() - Duration::from_secs(10);
                }
            } else if let Some(root) =
                unsafe { Owned::from_create(AXUIElementCreateApplication(pid)) }
            {
                self.apps.insert(
                    pid,
                    App {
                        meta,
                        root,
                        observer: None,
                        pending: None,
                        seen: HashSet::new(),
                        windows: HashMap::new(),
                        stale: true,
                        last: Instant::now() - Duration::from_secs(10),
                        focus: None,
                        coverage_complete: true,
                    },
                );
            }
        }
        self.apps.retain(|pid, _| live.contains(pid));
        self.queue.set_live(self.apps.keys().copied());
        let valid: HashSet<_> = self
            .apps
            .values()
            .flat_map(|a| a.windows.keys().copied())
            .collect();
        self.elements.retain(|id, _| valid.contains(id));
    }
    fn poll(&mut self) {
        unsafe {
            CFRunLoopRunInMode(kCFRunLoopDefaultMode, 0.0, true);
        }
        let all = self.events.wake.swap(false, Ordering::Relaxed);
        if all || Instant::now() >= self.next_meta {
            self.reconcile();
        }
        for (pid, app) in &self.apps {
            let event = app
                .observer
                .as_ref()
                .is_some_and(|o| o.wake.swap(false, Ordering::Relaxed));
            let interval = if app.observer.is_some() {
                Duration::from_secs(5)
            } else if *pid == self.front {
                Duration::from_secs(1)
            } else {
                Duration::from_secs(3)
            };
            if all || event || app.last.elapsed() >= interval {
                self.queue.notify(*pid);
            }
        }
    }
    fn slice(&mut self) -> bool {
        let Some(pid) = self.queue.next(Instant::now()) else {
            return false;
        };
        let started = Instant::now();
        let app = self.apps.get_mut(&pid).unwrap();
        let (_, status) = ipc_budget::run(SLICE, || {
            if app.observer.is_none() {
                app.observer = Observer::new(pid, app.root.0);
            }
            if app.pending.is_none() {
                app.focus = attr(app.root.0, ns_string!("AXFocusedWindow"));
                let Some(windows) = attr(app.root.0, ns_string!("AXWindows")) else {
                    ipc_budget::record(-25205, false);
                    return;
                };
                if !ipc_budget::healthy() {
                    return;
                }
                let raw = array(windows.0);
                app.coverage_complete = raw.len() <= MAX_PER_APP;
                if raw.len() > MAX_PER_APP {
                    self.truncated = true;
                }
                app.pending = Some(
                    raw.into_iter()
                        .take(MAX_PER_APP)
                        .filter_map(|p| unsafe { Owned::from_borrowed(p) })
                        .collect(),
                );
                app.seen.clear();
            }
            while !ipc_budget::expired() {
                let Some(el) = app.pending.as_mut().unwrap().pop_front() else {
                    break;
                };
                let role = text_attr(el.0, ns_string!("AXSubrole"));
                if !ipc_budget::healthy() {
                    if ipc_budget::discard_destroyed_element() {
                        continue;
                    }
                    app.pending.as_mut().unwrap().push_front(el);
                    break;
                }
                if ![
                    "AXStandardWindow",
                    "AXDialog",
                    "AXSystemDialog",
                    "AXFloatingWindow",
                ]
                .contains(&role.as_str())
                {
                    continue;
                }
                let b = geometry(el.0);
                let title = text_attr(el.0, ns_string!("AXTitle"));
                let id = unsafe { crate::private_api::window_id(el.0) }
                    .or_else(|| match_id(pid, &title, b, &self.cg));
                let minimized = bool_attr(el.0, ns_string!("AXMinimized"));
                let fullscreen = bool_attr(el.0, ns_string!("AXFullScreen"));
                if !ipc_budget::healthy() {
                    if ipc_budget::discard_destroyed_element() {
                        continue;
                    }
                    app.pending.as_mut().unwrap().push_front(el);
                    break;
                }
                let Some(id) = id.filter(|id| *id != 0) else {
                    self.truncated = true;
                    app.coverage_complete = false;
                    continue;
                };
                if b.2 < 40.0 || b.3 < 30.0 {
                    continue;
                }
                let ws = if self.spaces.is_empty() {
                    vec![]
                } else {
                    window_spaces(id)
                };
                let on_space = if ws.is_empty() {
                    self.visible.contains(&id) || minimized || app.meta.hidden
                } else {
                    ws.iter().any(|s| self.spaces.contains(s))
                };
                let focused = pid == self.front
                    && app
                        .focus
                        .as_ref()
                        .is_some_and(|f| unsafe { CFEqual(f.0, el.0) })
                    && !minimized
                    && !app.meta.hidden;
                app.seen.insert(id);
                if self.elements.len() < 4096 || self.elements.contains_key(&id) {
                    app.windows.insert(
                        id,
                        Window {
                            id,
                            pid,
                            bundle: app.meta.bundle.clone(),
                            app: app.meta.name.clone(),
                            path: app.meta.path.clone(),
                            title,
                            minimized,
                            fullscreen,
                            hidden: app.meta.hidden,
                            focused,
                            subordinate: role != "AXStandardWindow",
                            all_spaces: ws.len() > 1,
                            on_space,
                            tabbed_hidden: !minimized
                                && !app.meta.hidden
                                && on_space
                                && !self.visible.contains(&id)
                                && !fullscreen,
                            x: b.0,
                            y: b.1,
                            width: b.2,
                            height: b.3,
                            stale: false,
                        },
                    );
                    if let Some(observer) = &mut app.observer {
                        observer.window(id, el.0);
                    }
                    self.elements.insert(id, el);
                } else {
                    self.truncated = true;
                    app.coverage_complete = false;
                }
            }
            if app.pending.as_ref().is_some_and(|p| p.is_empty())
                && ipc_budget::healthy()
                && app.coverage_complete
                && let Some(observer) = &mut app.observer
            {
                observer.prune(&app.seen);
            }
        });
        let failed = status.error.is_some();
        // A destroyed AX element cannot recover. Re-enumerate after backoff,
        // preserving the last complete cache instead of retrying it forever.
        if failed {
            app.pending = None;
            app.focus = None;
        }
        let continuing = app.pending.as_ref().is_some_and(|p| !p.is_empty());
        let finished = app.pending.is_some() && !continuing && !failed;
        if finished {
            retain_complete(&mut app.windows, &app.seen, app.coverage_complete);
            app.pending = None;
            app.focus = None;
            app.stale = !app.coverage_complete;
            app.last = Instant::now();
            let valid: HashSet<_> = self
                .apps
                .values()
                .flat_map(|a| a.windows.keys().copied())
                .collect();
            self.elements.retain(|id, _| valid.contains(id));
        } else {
            app.stale = true;
        }
        self.status.slices += 1;
        self.status.max_slice_ms = self.status.max_slice_ms.max(started.elapsed().as_millis());
        if failed {
            self.status.timeouts += 1;
        }
        self.queue.finish(
            pid,
            continuing || failed || (!finished && status.exhausted),
            failed,
            Instant::now(),
        );
        true
    }
    fn value(&self) -> Snapshot {
        let mut status = self.status.clone();
        status.pending_apps = self.queue.pending();
        status.stale_apps = self.apps.values().filter(|a| a.stale).count();
        status.observer_apps = self.apps.values().filter(|a| a.observer.is_some()).count();
        status.incomplete = self.truncated || status.stale_apps > 0 || status.pending_apps > 0;
        let mut windows = Vec::with_capacity(self.apps.values().map(|a| a.windows.len()).sum());
        windows.extend(self.apps.values().flat_map(|a| {
            a.windows.values().map(|w| {
                let mut w = w.clone();
                w.stale = a.stale;
                w.focused = w.focused && w.pid == self.front;
                w
            })
        }));
        windows.sort_by_key(|w| w.id);
        Snapshot {
            windows,
            badges: self.badges.clone(),
            trusted: trusted(),
            screen_allowed: screen_allowed(),
            scan_ms: status.max_slice_ms,
            control_error: self.control_error.clone(),
            capabilities: {
                let mut capabilities = crate::private_api::capabilities();
                capabilities.private_spaces &= !self.spaces.is_empty();
                capabilities
            },
            discovery: status,
        }
    }
    fn badges(&mut self) {
        if self.badge_job.is_none() {
            self.next_badge = Instant::now() + Duration::from_secs(3);
            self.badge_job = BadgeJob::new();
        }
        let Some(job) = self.badge_job.as_mut() else {
            return;
        };
        let (_, status) = ipc_budget::run(SLICE, || job.slice());
        if status.error.is_some() {
            self.badge_job = None;
            return;
        }
        if job.pending.is_empty() {
            self.badges = std::mem::take(&mut job.result);
            self.badge_job = None;
        }
    }
}
fn retain_complete<T>(cache: &mut HashMap<u32, T>, seen: &HashSet<u32>, complete: bool) {
    if complete {
        cache.retain(|id, _| seen.contains(id));
    }
}
#[cfg(test)]
mod cache_tests {
    use super::*;
    #[test]
    fn ambiguous_or_truncated_generation_preserves_previous_windows() {
        let mut cache = HashMap::from([(1, "previous"), (2, "updated")]);
        let seen = HashSet::from([2]);
        retain_complete(&mut cache, &seen, false);
        assert_eq!(cache.len(), 2);
        retain_complete(&mut cache, &seen, true);
        assert_eq!(cache, HashMap::from([(2, "updated")]));
    }
}
pub(super) fn worker(rx: Receiver<Command>, tx: std::sync::mpsc::SyncSender<Snapshot>) {
    worker_core(rx, tx, None)
}
// Only the latest activation can recover. This holds IDs and times, never AX
// objects or images. A later control or a change to a third app cancels it.
struct Activation {
    id: u32,
    pid: i32,
    source: i32,
    next: Instant,
    deadline: Instant,
    attempts: u8,
}
impl Activation {
    fn new(id: u32, pid: i32, source: i32, now: Instant) -> Self {
        Self {
            id,
            pid,
            source,
            next: now + Duration::from_millis(40),
            deadline: now + Duration::from_secs(1),
            attempts: 1,
        }
    }
    fn permits_front(&self, front: i32) -> bool {
        front == self.pid || front == self.source || front == std::process::id() as i32
    }
    fn can_retry(&self, now: Instant) -> bool {
        now < self.deadline && self.attempts < 4
    }
    fn retried(&mut self, now: Instant) {
        self.attempts += 1;
        self.next = now + Duration::from_millis(40 << (self.attempts - 1));
    }
}
fn front_pid() -> i32 {
    objc2_app_kit::NSWorkspace::sharedWorkspace()
        .frontmostApplication()
        .map_or(0, |app| app.processIdentifier())
}
fn activation_retryable(status: ipc_budget::Status) -> bool {
    // AXCannotComplete can mean that the action arrived but its reply was late.
    // Restore/raise are idempotent. Never repeat close, minimize, hide or quit.
    status.error.is_none_or(|error| error == -25204)
}
fn advance_activation(service: &mut Service, pending: &mut Option<Activation>) {
    let Some(request) = pending.as_mut().filter(|r| Instant::now() >= r.next) else {
        return;
    };
    let front = front_pid();
    if !request.permits_front(front) {
        *pending = None;
        return;
    }
    let Some(window) = service.elements.get(&request.id) else {
        *pending = None;
        return;
    };
    let (focused, _) = ipc_budget::run(Duration::from_millis(20), || {
        front == request.pid
            && service.apps.get(&request.pid).is_some_and(|app| {
                attr(app.root.0, ns_string!("AXFocusedWindow")).is_some_and(|focus| {
                    // SAFETY: both retained AX elements stay on this worker
                    // and remain alive throughout the identity comparison.
                    unsafe { CFEqual(focus.0, window.0) }
                })
            })
    });
    if focused {
        *pending = None;
        service.queue.notify_all();
        return;
    }
    if !request.can_retry(Instant::now()) {
        service.control_error = Some("The window did not become active".into());
        *pending = None;
        return;
    }
    let (result, status) = ipc_budget::run(Duration::from_millis(120), || {
        execute(Command::Activate(request.id, false), &service.elements)
    });
    if activation_retryable(status) {
        request.retried(Instant::now());
    } else {
        service.control_error = result
            .err()
            .or_else(|| Some(format!("Window activation did not complete: {status:?}")));
        *pending = None;
    }
    service.queue.notify_all();
}
fn worker_core(
    rx: Receiver<Command>,
    tx: std::sync::mpsc::SyncSender<Snapshot>,
    fixture: Option<i32>,
) {
    let mut service = autoreleasepool(|_| Service::new());
    service.fixture_pid = fixture;
    let mut next_publish = Instant::now();
    let mut activation: Option<Activation> = None;
    let mut deferred_closes: HashMap<u32, (Instant, Instant)> = HashMap::new();
    // Opening a cold app and Dock changes can involve LaunchServices waits.
    // Their bounded queue never holds up the window-control actor.
    let (slow_tx, slow_rx) = std::sync::mpsc::sync_channel(4);
    let (result_tx, result_rx) = std::sync::mpsc::sync_channel(8);
    std::thread::spawn(move || {
        while let Ok(command) = slow_rx.recv() {
            if crate::runtime::stopping() {
                break;
            }
            let result = autoreleasepool(|_| {
                ipc_budget::run(Duration::from_secs(4), || execute(command, &HashMap::new())).0
            });
            let _ = result_tx.try_send(result);
        }
    });
    while !crate::runtime::stopping() {
        while let Ok(result) = result_rx.try_recv() {
            if let Err(error) = result {
                service.control_error = Some(error);
            }
        }
        // Always service controls before the next bounded discovery slice.
        match rx.recv_timeout(Duration::from_millis(10)) {
            Ok(Command::Stop) => break,
            Ok(Command::Probe(reply)) => {
                let _ = reply.send(Instant::now());
            }
            Ok(command @ (Command::Launch(..) | Command::Dock(_))) => {
                if matches!(command, Command::Launch(..)) {
                    activation = None;
                }
                if slow_tx.try_send(command).is_err() {
                    service.control_error =
                        Some("Application launch queue is busy; try again shortly".into());
                }
            }
            Ok(Command::Close(id)) if deferred_closes.contains_key(&id) => {}
            Ok(command) => {
                // Discovery and automatic resize cannot supersede a click.
                // Every other user control cancels recovery of an older target.
                if !matches!(command, Command::Resize(..)) {
                    activation = None;
                }
                let requested = match command {
                    Command::Activate(id, false) => service
                        .apps
                        .iter()
                        .find(|(_, app)| app.windows.contains_key(&id))
                        .map(|(pid, _)| Activation::new(id, *pid, front_pid(), Instant::now())),
                    _ => None,
                };
                let (result, status) = autoreleasepool(|_| {
                    ipc_budget::run(Duration::from_millis(160), || {
                        execute(command, &service.elements)
                    })
                });
                service.control_error = result.as_ref().err().cloned().or_else(|| {
                    (status.exhausted || status.error.is_some())
                        .then(|| format!("Window control did not complete: {status:?}"))
                });
                if requested.is_some() && activation_retryable(status) {
                    activation = requested.map(|mut request| {
                        // Keep recovery out of the first command's time slice.
                        request.next = Instant::now() + Duration::from_millis(40);
                        request
                    });
                    // A transient failure remains pending until actual focus or
                    // a bounded terminal failure, instead of discarding the click.
                    service.control_error = None;
                }
                if let Ok(Some(id)) = result {
                    if deferred_closes.len() < 32 || deferred_closes.contains_key(&id) {
                        deferred_closes.entry(id).or_insert((
                            Instant::now() + Duration::from_millis(50),
                            Instant::now() + Duration::from_secs(2),
                        ));
                    } else {
                        service.control_error = Some("Minimized close queue is full".into());
                    }
                }
                service.queue.notify_all();
            }
            Err(std::sync::mpsc::RecvTimeoutError::Disconnected) => break,
            _ => {}
        }
        autoreleasepool(|_| advance_activation(&mut service, &mut activation));
        // Wait for native restore completion without blocking other controls.
        if let Some((id, (_, deadline))) = deferred_closes
            .iter()
            .find(|(_, (next, _))| Instant::now() >= *next)
            .map(|(id, timing)| (*id, *timing))
        {
            deferred_closes.remove(&id);
            if !service.elements.contains_key(&id) {
                service.queue.notify_all();
            } else if Instant::now() >= deadline {
                service.control_error =
                    Some("Minimized window did not become available for close".into());
            } else {
                let (result, status) = autoreleasepool(|_| {
                    ipc_budget::run(Duration::from_millis(160), || {
                        execute(Command::CloseRestored(id), &service.elements)
                    })
                });
                match result {
                    Ok(Some(_)) => {
                        deferred_closes
                            .insert(id, (Instant::now() + Duration::from_millis(50), deadline));
                    }
                    Ok(None) if !status.exhausted && status.error.is_none() => {
                        service.queue.notify_all();
                    }
                    Err(error) => service.control_error = Some(error),
                    _ => service.control_error = Some("Cannot close the restored window".into()),
                }
            }
        }
        autoreleasepool(|_| {
            service.poll();
            if !service.slice()
                && (service.badge_job.is_some() || Instant::now() >= service.next_badge)
            {
                service.badges();
            }
        });
        if Instant::now() >= next_publish {
            next_publish = Instant::now() + Duration::from_millis(150);
            match tx.try_send(service.value()) {
                Ok(()) => service.control_error = None,
                Err(std::sync::mpsc::TrySendError::Disconnected(_)) => break,
                _ => {}
            }
        }
    }
}
pub(super) fn check_fixture_activation(pid: i32) -> bool {
    let inactive_source = !NSRunningApplication::currentApplication().isActive();
    let (tx, rx) = std::sync::mpsc::sync_channel(8);
    let (snap_tx, snap_rx) = std::sync::mpsc::sync_channel(1);
    let worker = std::thread::spawn(move || worker_core(rx, snap_tx, Some(pid)));
    let mut snapshot = Snapshot::default();
    let pump = |snapshot: &mut Snapshot| {
        objc2_foundation::NSRunLoop::currentRunLoop().runUntilDate(
            &objc2_foundation::NSDate::dateWithTimeIntervalSinceNow(0.02),
        );
        while let Ok(value) = snap_rx.try_recv() {
            *snapshot = value;
        }
    };
    let discovery = Instant::now();
    while snapshot.windows.iter().filter(|w| w.pid == pid).count() < 2
        && discovery.elapsed() < Duration::from_secs(3)
    {
        pump(&mut snapshot);
    }
    let ids: Vec<_> = snapshot
        .windows
        .iter()
        .filter(|w| w.pid == pid && w.bundle == "io.sharif.taskbarrust.interactionfixture")
        .map(|w| w.id)
        .take(2)
        .collect();
    let mut successes = 0;
    let mut max_ms = 0.0f64;
    let mut error = None;
    for id in ids.iter().cycle().take(if ids.len() == 2 { 4 } else { 0 }) {
        let started = Instant::now();
        if tx.try_send(Command::Activate(*id, false)).is_err() {
            error = Some("Fixture control queue is unavailable".into());
            break;
        }
        loop {
            pump(&mut snapshot);
            if snapshot.control_error.is_some() {
                error.clone_from(&snapshot.control_error);
                break;
            }
            // Use the actor's focus report and a fresh owner query. Each case
            // sends one command through the same recovery path as a UI click.
            if front_pid() == pid && snapshot.windows.iter().any(|w| w.id == *id && w.focused) {
                successes += 1;
                max_ms = max_ms.max(started.elapsed().as_secs_f64() * 1000.0);
                break;
            }
            if started.elapsed() >= Duration::from_secs(2) {
                error = Some("Fixture window did not become active".into());
                break;
            }
        }
        if error.is_some() {
            break;
        }
    }
    let mut latest_target_wins = false;
    if successes == 4 {
        // A queued click supersedes recovery of the previous window. Wait past
        // the old retry interval to catch a later unwanted focus change.
        let queued = tx.try_send(Command::Activate(ids[0], false)).is_ok()
            && tx.try_send(Command::Activate(ids[1], false)).is_ok();
        let settle = Instant::now();
        while settle.elapsed() < Duration::from_secs(1) {
            pump(&mut snapshot);
        }
        latest_target_wins = queued
            && snapshot.control_error.is_none()
            && front_pid() == pid
            && snapshot.windows.iter().any(|w| w.id == ids[1] && w.focused);
    }
    let _ = tx.send(Command::Stop);
    drop(tx);
    let _ = worker.join();
    let passed = inactive_source && successes == 4 && latest_target_wins && cf_counts().0 == 0;
    println!(
        "{}",
        serde_json::json!({"passed":passed,"inactive_sender":inactive_source,
        "fixture_windows":ids.len(),"activation_successes":successes,"max_ms":max_ms,
        "latest_target_wins":latest_target_wins,"error":error,"cf_live":cf_counts().0})
    );
    passed
}
pub(super) fn snapshot(elements: &mut HashMap<u32, Owned>, budget: Duration) -> Snapshot {
    let mut service = Service::new();
    let start = Instant::now();
    service.reconcile();
    service.queue.notify_all();
    while start.elapsed() < budget && !crate::runtime::stopping() {
        if !service.slice() {
            break;
        }
    }
    let value = service.value();
    *elements = std::mem::take(&mut service.elements);
    value
}
pub(super) fn benchmark(seconds: u64) -> bool {
    let mut service = autoreleasepool(|_| Service::new());
    let start = Instant::now();
    let mut max_windows = 0;
    let mut last = Instant::now();
    while start.elapsed() < Duration::from_secs(seconds.min(600)) && !crate::runtime::stopping() {
        autoreleasepool(|_| {
            service.poll();
            service.slice();
        });
        if last.elapsed() >= Duration::from_secs(1) {
            last = Instant::now();
            let value = service.value();
            max_windows = max_windows.max(value.windows.len());
            println!(
                "{}",
                serde_json::json!({"seconds":start.elapsed().as_secs_f64(),"windows":value.windows.len(),"trusted":value.trusted,"screen_allowed":value.screen_allowed,"capabilities":value.capabilities,"discovery":value.discovery,"cf_live":cf_counts().0,"cf_peak":cf_counts().1})
            );
        }
        std::thread::sleep(Duration::from_millis(10));
    }
    let status = service.value().discovery;
    drop(service);
    let (live, peak) = cf_counts();
    println!(
        "{}",
        serde_json::json!({"completed":true,"max_windows":max_windows,"cf_live":live,"cf_peak":peak,"discovery":status})
    );
    trusted() && max_windows > 0 && live == 0
}

pub(super) fn latency(seconds: u64, fixture: Option<i32>) -> bool {
    let (tx, rx) = std::sync::mpsc::sync_channel(8);
    let (snap_tx, snap_rx) = std::sync::mpsc::sync_channel(1);
    let worker = std::thread::spawn(move || worker_core(rx, snap_tx, fixture));
    let start = Instant::now();
    let mut samples = Vec::new();
    let mut last = Snapshot::default();
    let mut failures = 0;
    while start.elapsed() < Duration::from_secs(seconds.clamp(3, 120)) {
        let (reply, answer) = std::sync::mpsc::channel();
        let request = Instant::now();
        if tx.try_send(Command::Probe(reply)).is_ok() {
            match answer.recv_timeout(Duration::from_secs(1)) {
                Ok(at) => samples.push(at.duration_since(request).as_secs_f64() * 1000.0),
                Err(_) => failures += 1,
            }
        } else {
            failures += 1;
        }
        while let Ok(value) = snap_rx.try_recv() {
            last = value;
        }
        std::thread::sleep(Duration::from_millis(100));
    }
    crate::runtime::stop();
    drop(tx);
    let _ = worker.join();
    samples.sort_by(f64::total_cmp);
    let max = samples.last().copied().unwrap_or(f64::INFINITY);
    let p95 = samples
        .get(samples.len() * 95 / 100)
        .copied()
        .unwrap_or(f64::INFINITY);
    println!(
        "{}",
        serde_json::json!({"probe_samples":samples.len(),"probe_failures":failures,"p95_ms":p95,"max_ms":max,"windows":last.windows.len(),"fixture_windows":last.windows.iter().filter(|w|Some(w.pid)==fixture).count(),"discovery":last.discovery,"cf_live":cf_counts().0})
    );
    !samples.is_empty() && failures == 0 && max < 200.0 && cf_counts().0 == 0
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn activation_recovery_is_bounded_and_respects_a_different_front_app() {
        let now = Instant::now();
        let mut activation = Activation::new(42, 7, 8, now);
        assert!(activation.permits_front(7));
        assert!(activation.permits_front(8));
        assert!(activation.permits_front(std::process::id() as i32));
        assert!(!activation.permits_front(9));
        for _ in 0..3 {
            assert!(activation.can_retry(now));
            activation.retried(now);
        }
        assert!(!activation.can_retry(now));
        assert!(!Activation::new(42, 7, 8, now).can_retry(now + Duration::from_secs(1)));
        assert!(activation_retryable(ipc_budget::Status {
            error: Some(-25204),
            ..Default::default()
        }));
        for error in [-25202, -25205, -25211] {
            assert!(!activation_retryable(ipc_budget::Status {
                error: Some(error),
                ..Default::default()
            }));
        }
    }
    #[test]
    fn public_identity_rejects_ambiguous_windows() {
        let a = PublicWindow {
            id: 1,
            pid: 4,
            title: "same".into(),
            bounds: (0., 0., 500., 500.),
        };
        let mut b = a.clone();
        b.id = 2;
        assert_eq!(match_id(4, "same", a.bounds, &[a.clone(), b]), None);
        assert_eq!(match_id(4, "", a.bounds, &[a]), Some(1));
    }
}
