//! Native AppKit controls backed by weak references to Rust state.
use crate::{
    config::{Config, Pin},
    platform::{self, Application, Command, Snapshot, Window},
};
use objc2::{
    AnyThread, DefinedClass, MainThreadOnly, define_class, msg_send,
    rc::{Retained, autoreleasepool},
    runtime::{AnyObject, ProtocolObject, Sel},
    sel,
};
use objc2_app_kit::*;
use objc2_foundation::{
    MainThreadMarker, NSArray, NSNotification, NSObject, NSObjectProtocol, NSPoint, NSRect, NSSize,
    NSString, NSTimer, NSURL,
};
use serde::{Deserialize, Serialize};
use std::{
    cell::{Cell, RefCell},
    collections::{HashMap, HashSet},
    ptr::NonNull,
    rc::{Rc, Weak},
    sync::mpsc,
    time::{Duration, Instant},
};
mod active_feedback;
mod brand;
mod chrome_profiles;
mod hover_close;
mod preview_cache;
mod preview_frame;
mod preview_lifecycle;
mod qa;
mod related;
mod sort_order;
type Shared = Rc<RefCell<State>>;
type WeakState = Weak<RefCell<State>>;
#[derive(Clone, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
enum Action {
    Window(u32),
    Bubble(u32),
    Tab(u32, u64),
    CloseTab(u32, u64),
    RelatedMore(u32),
    Raise(u32),
    Preview(u32),
    ScrollTasks(u32, bool),
    ClosePreview,
    Pin(String, String),
    Start(u32),
    Sort,
    AddOrderApp,
    AppendOrderApp(String),
    MoveOrderApp(String, bool),
    RemoveOrderApp(String),
    UseTaskOrder,
    UseDockOrder,
    Preferences(usize),
    Toggle(String),
    Cycle(String),
    Minimize(u32),
    Close(u32),
    Fullscreen(u32),
    HideApp(i32),
    QuitApp(i32),
    Keep(String),
    Unpin(String),
    Blacklist(String),
    AddBlacklist,
    RemoveBlacklist(String),
    PinBehavior(String, String),
    HideBar(u32, bool),
    ShowBars,
    HideBars,
    OpenShortcuts,
    InstallShortcut(crate::system_actions::SystemAction),
    Accessibility,
    ScreenPermission,
    Reset,
    Diagnostics,
    Help,
    CheckUpdates,
    Quit,
}
struct Bar {
    display: u32,
    frame: NSRect,
    panel: Retained<NSPanel>,
    root: Retained<BarView>,
    tasks: Retained<NSView>,
    scroll: Retained<TaskScrollView>,
    buttons: HashMap<Action, Retained<ActionButton>>,
}
struct Preview {
    panel: Retained<NSPanel>,
    frame: Retained<preview_frame::PreviewFrame>,
    image: Retained<NSImageView>,
    title: Retained<NSTextField>,
    close: Retained<ActionButton>,
}
struct Preferences {
    panel: Retained<NSWindow>,
    body: Retained<NSView>,
    page: usize,
    targets: Vec<Retained<NSObject>>,
}
struct StartMenu {
    panel: Retained<NSPanel>,
    search: Retained<NSSearchField>,
    body: Retained<NSView>,
    query: String,
    purpose: PickerPurpose,
    choices: Vec<Action>,
    selected: usize,
}
#[derive(Clone, Copy, PartialEq, Eq)]
enum PickerPurpose {
    Launch,
    Blacklist,
    AppOrder,
}
struct GuiSmoke {
    remaining: usize,
    total: usize,
    last: Instant,
}
struct State {
    gui_smoke: Option<GuiSmoke>,
    ui_benchmark: Option<usize>,
    hover_benchmark: Option<qa::HoverBenchmark>,
    hover_close: Option<hover_close::Service>,
    performance: qa::Performance,
    config: Config,
    snapshot: Snapshot,
    pending_focus: Option<active_feedback::PendingFocus>,
    pending_tab: Option<(u32, u64, Instant)>,
    pending_system_sort: bool,
    received_snapshot: bool,
    bars: Vec<Bar>,
    icons: HashMap<String, Retained<NSImage>>,
    profile_badges: chrome_profiles::Service,
    order: Vec<u32>,
    activity: crate::window_order::Activity,
    sort_order_error: Option<String>,
    known_spaces: HashMap<u32, bool>,
    hidden_now: HashSet<u32>,
    preferences: Option<Preferences>,
    start: Option<StartMenu>,
    preview: Option<Preview>,
    hover: Option<(u32, u32, Instant)>,
    hover_anchor: NSRect,
    preview_cache: preview_cache::PreviewCache<platform::Image>,
    image_rx: mpsc::Receiver<Result<(u32, platform::Image), String>>,
    image_tx: mpsc::Sender<Result<(u32, platform::Image), String>>,
    capture_busy: bool,
    capture_source: Option<u32>,
    prefetch_after: Instant,
    prefetch_retry: std::collections::VecDeque<(u32, Instant)>,
    capture_started: Instant,
    capture_gate: crate::runtime::PreviewGate,
    preview_id: Option<u32>,
    captures: u64,
    capture_errors: u64,
    preview_until: Option<Instant>,
    tx: mpsc::SyncSender<Command>,
    rx: mpsc::Receiver<Snapshot>,
    apps: Vec<Application>,
    apps_rx: mpsc::Receiver<Vec<Application>>,
    delegate: Option<NonNull<Delegate>>,
    status: Option<Retained<NSStatusItem>>,
    timer: Option<Retained<NSTimer>>,
    interaction_timer: Option<Retained<NSTimer>>,
    dirty: bool,
    last_screens: Vec<(u32, NSRect)>,
    error: String,
    last_metrics: Instant,
    queue_error: RefCell<Option<String>>,
    updater: crate::updater::Service,
    observed_dark: bool,
    catalog_busy: bool,
    last_catalog: Instant,
}
impl State {
    fn delegate(&self) -> &Delegate {
        let delegate = self
            .delegate
            .expect("delegate installed before UI construction");
        // SAFETY: run_mode owns the retained delegate throughout app.run; all
        // timers and callback sources are invalidated before that owner drops.
        unsafe { delegate.as_ref() }
    }
    fn command(&self, command: Command) -> bool {
        // Hidden GUI tests exercise production handlers without touching user windows.
        if self.gui_smoke.is_some() {
            self.performance
                .commands
                .set(self.performance.commands.get() + 1);
            return true;
        }
        let app = NSApplication::sharedApplication(MainThreadMarker::new().expect("UI thread"));
        if app.isActive() {
            match &command {
                Command::Activate(id, false) => {
                    if let Some(window) = self.snapshot.windows.iter().find(|w| w.id == *id)
                        && let Some(target) =
                            NSRunningApplication::runningApplicationWithProcessIdentifier(
                                window.pid,
                            )
                    {
                        app.yieldActivationToApplication(&target);
                    }
                }
                Command::SelectTab(token) => {
                    if let Some(window) = self
                        .snapshot
                        .windows
                        .iter()
                        .find(|w| w.tabs.iter().any(|t| t.id == *token))
                        && let Some(target) =
                            NSRunningApplication::runningApplicationWithProcessIdentifier(
                                window.pid,
                            )
                    {
                        app.yieldActivationToApplication(&target);
                    }
                }
                Command::Launch(bundle, _) => {
                    app.yieldActivationToApplicationWithBundleIdentifier(&NSString::from_str(
                        bundle,
                    ));
                }
                _ => {}
            }
        }
        if let Err(error) = self.tx.try_send(command) {
            *self.queue_error.borrow_mut() =
                Some(format!("Window command queue is busy or stopped: {error}"));
            return false;
        }
        true
    }
    fn focused_id(&self) -> Option<u32> {
        self.pending_focus.map(|pending| pending.id).or_else(|| {
            self.snapshot
                .windows
                .iter()
                .find(|w| w.focused)
                .map(|w| w.id)
        })
    }
}
struct DelegateIvars {
    state: Shared,
}
define_class!(
 #[unsafe(super=NSObject)] #[thread_kind=MainThreadOnly] #[ivars=DelegateIvars] #[name = "TaskbarRustDelegate"] struct Delegate;
 unsafe impl NSObjectProtocol for Delegate {}
 unsafe impl NSTextFieldDelegate for Delegate {}
 unsafe impl NSSearchFieldDelegate for Delegate {}
 unsafe impl NSControlTextEditingDelegate for Delegate {
  #[unsafe(method(control:textView:doCommandBySelector:))]
  unsafe fn search_command(&self,_control:&NSControl,_view:&NSTextView,command:Sel)->bool {
   if command==sel!(moveDown:){move_start_selection(&self.ivars().state,1);true}
   else if command==sel!(moveUp:){move_start_selection(&self.ivars().state,-1);true}
   else if command==sel!(insertNewline:){activate_start_selection(&self.ivars().state);true}
   else if command==sel!(cancelOperation:){if let Some(m)=self.ivars().state.borrow_mut().start.take(){m.panel.close();}true}
   else{false}
  }
 }
 unsafe impl NSApplicationDelegate for Delegate {
  #[unsafe(method(applicationDidFinishLaunching:))]
  fn launched(&self,_n:&NSNotification){let s=&self.ivars().state;
   initialize_preview(s);hover_close::install(s);if s.borrow().gui_smoke.is_none(){s.borrow().command(Command::Dock(s.borrow().config.fully_hide_dock));}let timer=unsafe{NSTimer::scheduledTimerWithTimeInterval_target_selector_userInfo_repeats(0.1,self,sel!(tick:),None,true)};s.borrow_mut().timer=Some(timer);render(s);
   if s.borrow().gui_smoke.is_none(){let enabled=s.borrow().config.start_at_login;if let Err(e)=platform::login(enabled){s.borrow_mut().error=e;}}
  }
  #[unsafe(method(applicationWillTerminate:))]
  fn terminating(&self,_n:&NSNotification){let mut s=self.ivars().state.borrow_mut();if let Some(t)=s.timer.take(){t.invalidate();}
   if let Some(t)=s.interaction_timer.take(){t.invalidate();}s.hover_close.take();crate::runtime::stop();let _=s.tx.try_send(Command::Stop);if s.gui_smoke.is_none(){if let Err(e)=s.config.save(){s.error=e;}let _=crate::dock::shutdown_restore();}crate::capture::shutdown();}
  #[unsafe(method(applicationShouldHandleReopen:hasVisibleWindows:))]
  fn reopen(&self,_a:&NSApplication,_v:bool)->bool{show_preferences(&self.ivars().state,0);true}
  #[unsafe(method(application:openURLs:))]
  fn open_urls(&self,_app:&NSApplication,urls:&NSArray<NSURL>){
   for url in urls {
    if let Some(url)=url.absoluteString() && let Some(action)=crate::system_actions::SystemAction::from_url(&url.to_string()) {
     system_action(&self.ivars().state,action);
    }
   }
  }
 }
 impl Delegate {
  #[unsafe(method(tick:))] fn tick(&self,_t:&NSTimer){tick(&self.ivars().state);}
  #[unsafe(method(interactionTick:))] fn interaction_tick(&self,_t:&NSTimer){autoreleasepool(|_| interaction_tick(&self.ivars().state));}
  #[unsafe(method(menuAction:))] fn menu_action(&self,sender:&NSMenuItem){if let Some(o)=sender.representedObject()&& let Some(s)=o.downcast_ref::<NSString>()&& let Ok(a)=serde_json::from_str(&s.to_string()){dispatch(&self.ivars().state,a);}}
  #[unsafe(method(statusAction:))] fn status_action(&self,_s:&AnyObject){show_preferences(&self.ivars().state,0);}
  #[unsafe(method(startSearch:))] fn start_search(&self,_field:&NSSearchField){activate_start_selection(&self.ivars().state);}
 }
);
impl Delegate {
    fn new(m: MainThreadMarker, state: Shared) -> Retained<Self> {
        unsafe {
            msg_send![
                super(Self::alloc(m).set_ivars(DelegateIvars { state })),
                init
            ]
        }
    }
}
#[derive(Clone, Copy, PartialEq)]
struct WindowButtonStyle {
    focused: bool,
    font_size: f64,
}
struct ButtonIvars {
    state: WeakState,
    action: Action,
    drag: Cell<Option<NSPoint>>,
    dragged: Cell<bool>,
    start_index: Cell<Option<usize>>,
    badge: Cell<bool>,
    window_style: Cell<Option<WindowButtonStyle>>,
    reserved_width: Cell<f64>,
    related_ids: RefCell<Vec<u32>>,
    hovered: Cell<bool>,
    profile_badge: RefCell<Option<Retained<NSImage>>>,
}
define_class!(
 #[unsafe(super=NSButton)] #[thread_kind=MainThreadOnly] #[ivars=ButtonIvars] #[name = "TaskbarRustActionButton"] struct ActionButton;
 unsafe impl NSObjectProtocol for ActionButton {}
 impl ActionButton {
  #[unsafe(method(drawRect:))] fn draw(&self,r:NSRect){
   let focused=self.ivars().window_style.get().is_some_and(|style|style.focused);
   let selected=self.ivars().start_index.get().is_some_and(|index|self.ivars().state.upgrade().is_some_and(|s|s.try_borrow().ok().is_some_and(|s|s.start.as_ref().is_some_and(|m|m.selected==index))));
   let bubble=matches!(self.ivars().action,Action::Bubble(_) | Action::Tab(..) | Action::RelatedMore(_));
   if bubble{let color=if focused{NSColor::controlAccentColor()}else{NSColor::secondaryLabelColor()};color.colorWithAlphaComponent(if self.ivars().hovered.get(){0.28}else{0.15}).set();NSBezierPath::bezierPathWithRoundedRect_xRadius_yRadius(self.bounds(),self.bounds().size.height/2.0,self.bounds().size.height/2.0).fill();}
   else if focused||selected{NSColor::controlAccentColor().colorWithAlphaComponent(0.3).set();NSBezierPath::bezierPathWithRoundedRect_xRadius_yRadius(self.bounds(),4.0,4.0).fill();}
   NSGraphicsContext::saveGraphicsState_class();
   let inset=self.ivars().reserved_width.get();
   if inset>0.0{NSBezierPath::bezierPathWithRect(rect(0.0,0.0,(self.bounds().size.width-inset).max(0.0),self.bounds().size.height)).addClip();}
   if inset>0.0&&let Some(cell)=self.cell(){cell.drawWithFrame_inView(rect(0.0,0.0,(self.bounds().size.width-inset).max(0.0),self.bounds().size.height),self);}
   else{unsafe{let _:()=msg_send![super(self),drawRect:r];}}
   NSGraphicsContext::restoreGraphicsState_class();
   chrome_profiles::draw(self);
   if self.ivars().badge.get(){NSColor::systemRedColor().set();let bounds=self.bounds();NSBezierPath::bezierPathWithOvalInRect(rect(bounds.size.width-self.ivars().reserved_width.get()-9.0,bounds.size.height-9.0,6.0,6.0)).fill();}
  }
  #[unsafe(method(pressed:))] fn pressed(&self,_sender:&AnyObject){if let Some(s)=self.ivars().state.upgrade(){dispatch(&s,self.ivars().action.clone());}}
  #[unsafe(method(acceptsFirstMouse:))] fn accepts_first_mouse(&self,_e:Option<&NSEvent>)->bool{true}
  #[unsafe(method(mouseDownCanMoveWindow))] fn mouse_down_can_move_window(&self)->bool{false}
  #[unsafe(method(resetCursorRects))] fn reset_cursor_rects(&self){self.addCursorRect_cursor(self.visibleRect(),&NSCursor::arrowCursor());}
  #[unsafe(method(cursorUpdate:))] fn cursor_update(&self,_e:&NSEvent){NSCursor::arrowCursor().set();}
  #[unsafe(method(mouseMoved:))] fn moved(&self,_e:&NSEvent){NSCursor::arrowCursor().set();}
  #[unsafe(method(mouseEntered:))] fn entered(&self,_e:&NSEvent){NSCursor::arrowCursor().set();self.ivars().hovered.set(true);NSView::setNeedsDisplay(self,true);if let Some(s)=self.ivars().state.upgrade(){match self.ivars().action {Action::Window(id)|Action::Bubble(id)=>if let Some(win)=self.window(){let r=win.convertRectToScreen(self.convertRect_toView(self.bounds(),None));begin_hover(&s,id,win.windowNumber(),r);},Action::Tab(..)=>{hide_preview(&s);s.borrow_mut().hover=None;},_=>{}}}}
  #[unsafe(method(mouseExited:))] fn exited(&self,_e:&NSEvent){self.ivars().hovered.set(false);NSView::setNeedsDisplay(self,true);if let Some(s)=self.ivars().state.upgrade()&& let Ok(mut s)=s.try_borrow_mut()&& matches!(self.ivars().action,Action::Window(id)|Action::Bubble(id) if s.hover.is_some_and(|h|h.0==id)){s.hover=None;}}
  #[unsafe(method(rightMouseDown:))] fn right_down(&self,e:&NSEvent){if let Some(s)=self.ivars().state.upgrade(){let m=context_menu(&s,Some(&self.ivars().action));{NSMenu::popUpContextMenu_withEvent_forView(&m,e,self);}}}
  #[unsafe(method(otherMouseDown:))] fn other_down(&self,e:&NSEvent){if e.buttonNumber()==2&& let Some(s)=self.ivars().state.upgrade()&& s.borrow().config.middle_closes{match self.ivars().action {Action::Window(id)|Action::Bubble(id)=>dispatch(&s,Action::Close(id)),Action::Tab(id,index)=>dispatch(&s,Action::CloseTab(id,index)),_=>{}}}}
  #[unsafe(method(mouseDown:))] fn down(&self,e:&NSEvent){NSCursor::arrowCursor().set();if !self.isEnabled(){return;}self.ivars().drag.set(Some(e.locationInWindow()));self.ivars().dragged.set(false);self.highlight(true);}
  #[unsafe(method(mouseDragged:))] fn dragged(&self,e:&NSEvent){NSCursor::arrowCursor().set();if self.can_drag()&& let Some(start)=self.ivars().drag.get(){let p=e.locationInWindow();if (p.x-start.x).abs()>5.0||(p.y-start.y).abs()>5.0{self.ivars().dragged.set(true);}}}
  #[unsafe(method(mouseUp:))] fn up(&self,e:&NSEvent){NSCursor::arrowCursor().set();self.highlight(false);if self.ivars().drag.take().is_none()||!self.isEnabled(){return;}
   if let Some(s)=self.ivars().state.upgrade(){let reordered=self.ivars().dragged.replace(false)&&reorder(&s,&self.ivars().action,e.locationInWindow().x,self.window().map(|w|w.windowNumber()).unwrap_or(0));if !reordered&& contains(self.bounds(),self.convertPoint_fromView(e.locationInWindow(),None)){dispatch(&s,self.ivars().action.clone());}}}
 }
);
impl ActionButton {
    fn set_window_focus(&self, focused: bool, font_size: f64) {
        let style = WindowButtonStyle { focused, font_size };
        if self.ivars().window_style.replace(Some(style)) == Some(style) {
            return;
        }
        let font = if focused {
            NSFont::boldSystemFontOfSize(font_size)
        } else {
            NSFont::systemFontOfSize(font_size)
        };
        self.setFont(Some(&font));
        self.setBezelColor(focused.then(NSColor::selectedControlColor).as_deref());
        NSView::setNeedsDisplay(self, true);
    }
    fn can_drag(&self) -> bool {
        self.ivars().state.upgrade().is_some_and(|s| {
            let s = s.borrow();
            match self.ivars().action {
                Action::Window(_) => s.config.task_dragging,
                Action::Pin(..) => s.config.pin_dragging,
                _ => false,
            }
        })
    }
    fn new(s: &Shared, action: Action, title: &str, r: NSRect) -> Retained<Self> {
        s.borrow_mut().performance.buttons_created += 1;
        let m = MainThreadMarker::new().unwrap();
        let b: Retained<Self> = unsafe {
            msg_send![super(Self::alloc(m).set_ivars(ButtonIvars{state:Rc::downgrade(s),action,drag:Cell::new(None),dragged:Cell::new(false),start_index:Cell::new(None),badge:Cell::new(false),window_style:Cell::new(None),reserved_width:Cell::new(0.0),related_ids:RefCell::new(Vec::new()),hovered:Cell::new(false),profile_badge:RefCell::new(None)})),initWithFrame:r]
        };
        b.setTitle(&NSString::from_str(title));
        let accessible = if matches!(b.ivars().action, Action::Start(_)) {
            "Start".into()
        } else if matches!(b.ivars().action, Action::Sort) {
            "Sort windows".into()
        } else if matches!(b.ivars().action, Action::ClosePreview) {
            "Close previewed window".into()
        } else if title.is_empty() {
            match &b.ivars().action {
                Action::Pin(bundle, _) => s
                    .borrow()
                    .apps
                    .iter()
                    .find(|a| a.bundle == *bundle)
                    .map(|a| a.name.clone())
                    .unwrap_or_else(|| bundle.clone()),
                Action::Start(_) => "Start".into(),
                _ => "Taskbar action".into(),
            }
        } else {
            title.into()
        };
        unsafe {
            let _: () = msg_send![&*b,setAccessibilityLabel:&*NSString::from_str(&accessible)];
        }
        if matches!(
            b.ivars().action,
            Action::Window(_) | Action::Pin(..) | Action::Start(_)
        ) {
            b.setBordered(false);
            b.setAlignment(NSTextAlignment::Left);
        }
        unsafe {
            b.setTarget(Some(&b));
            b.setAction(Some(sel!(pressed:)));
        }
        b.setBezelStyle(NSBezelStyle::SmallSquare);
        b.setFont(Some(&NSFont::systemFontOfSize(13.0)));
        if matches!(
            b.ivars().action,
            Action::Bubble(_) | Action::Tab(..) | Action::RelatedMore(_)
        ) {
            b.setBordered(false);
            b.setAlignment(NSTextAlignment::Center);
            b.setUsesSingleLineMode(true);
        }
        if matches!(b.ivars().action, Action::Window(_) | Action::Pin(..)) {
            b.setImageScaling(NSImageScaling::ScaleProportionallyUpOrDown);
        }
        if matches!(b.ivars().action, Action::Sort) {
            b.setBordered(true);
            b.setImage(Some(&sort_order::button_icon()));
            b.setImagePosition(NSCellImagePosition::ImageLeft);
        }
        if matches!(
            b.ivars().action,
            Action::Window(_) | Action::Pin(..) | Action::Start(_) | Action::Sort
        ) {
            if let Some(cell) = b.cell() {
                cell.setWraps(false);
            }
            b.setUsesSingleLineMode(true);
            b.setLineBreakMode(NSLineBreakMode::ByTruncatingTail);
        }
        b
    }
}
struct BarIvars {
    state: WeakState,
    display: u32,
    tracking: RefCell<Option<Retained<NSTrackingArea>>>,
}
define_class!(
 #[unsafe(super=NSView)] #[thread_kind=MainThreadOnly] #[ivars=BarIvars] #[name = "TaskbarRustBarView"] struct BarView;
 unsafe impl NSObjectProtocol for BarView {}
 impl BarView {
  #[unsafe(method(resetCursorRects))] fn reset_cursor_rects(&self){self.addCursorRect_cursor(self.visibleRect(),&NSCursor::arrowCursor());}
  #[unsafe(method(cursorUpdate:))] fn cursor_update(&self,_e:&NSEvent){NSCursor::arrowCursor().set();}
  #[unsafe(method(mouseEntered:))] fn entered(&self,_e:&NSEvent){NSCursor::arrowCursor().set();}
  #[unsafe(method(mouseMoved:))] fn moved(&self,_e:&NSEvent){NSCursor::arrowCursor().set();}
  #[unsafe(method(updateTrackingAreas))] fn update_tracking_areas(&self){update_arrow_tracking(self,&self.ivars().tracking);unsafe{let _:()=msg_send![super(self),updateTrackingAreas];}}
  #[unsafe(method(mouseDownCanMoveWindow))] fn mouse_down_can_move_window(&self)->bool{false}
  #[unsafe(method(rightMouseDown:))] fn right_down(&self,e:&NSEvent){if let Some(s)=self.ivars().state.upgrade(){let m=context_menu(&s,Some(&Action::HideBar(self.ivars().display,false)));{NSMenu::popUpContextMenu_withEvent_forView(&m,e,self);}}}
  #[unsafe(method(scrollWheel:))] fn scroll(&self,e:&NSEvent){NSCursor::arrowCursor().set();if let Some(s)=self.ivars().state.upgrade(){let hide={let s=s.borrow();(e.scrollingDeltaY()<0.0&&s.config.scroll_down_hides)||(e.scrollingDeltaY()>0.0&&s.config.scroll_up_hides)};if hide{dispatch(&s,Action::HideBar(self.ivars().display,true));}}}
 }
);
impl BarView {
    fn new(s: &Shared, d: u32, r: NSRect) -> Retained<Self> {
        let m = MainThreadMarker::new().unwrap();
        unsafe {
            msg_send![super(Self::alloc(m).set_ivars(BarIvars{state:Rc::downgrade(s),display:d,tracking:RefCell::new(None)})),initWithFrame:r]
        }
    }
}
define_class!(
 #[unsafe(super=NSScrollView)] #[thread_kind=MainThreadOnly] #[ivars=BarIvars] #[name="TaskbarRustTaskScrollView"] struct TaskScrollView;
 unsafe impl NSObjectProtocol for TaskScrollView {}
 impl TaskScrollView {
  #[unsafe(method(resetCursorRects))] fn reset_cursor_rects(&self){self.addCursorRect_cursor(self.visibleRect(),&NSCursor::arrowCursor());}
  #[unsafe(method(cursorUpdate:))] fn cursor_update(&self,_e:&NSEvent){NSCursor::arrowCursor().set();}
  #[unsafe(method(mouseEntered:))] fn entered(&self,_e:&NSEvent){NSCursor::arrowCursor().set();}
  #[unsafe(method(mouseMoved:))] fn moved(&self,_e:&NSEvent){NSCursor::arrowCursor().set();}
  #[unsafe(method(mouseDownCanMoveWindow))] fn mouse_down_can_move_window(&self)->bool{false}
  #[unsafe(method(updateTrackingAreas))] fn update_tracking_areas(&self){update_arrow_tracking(self,&self.ivars().tracking);unsafe{let _:()=msg_send![super(self),updateTrackingAreas];}}
  #[unsafe(method(scrollWheel:))] fn scroll(&self,e:&NSEvent){
   if e.scrollingDeltaX()!=0.0 || e.modifierFlags().contains(NSEventModifierFlags::Shift){unsafe{let _:()=msg_send![super(self),scrollWheel:e];}}
   else if let Some(s)=self.ivars().state.upgrade(){let hide={let s=s.borrow();(e.scrollingDeltaY()<0.0&&s.config.scroll_down_hides)||(e.scrollingDeltaY()>0.0&&s.config.scroll_up_hides)};if hide{dispatch(&s,Action::HideBar(self.ivars().display,true));}}
   NSCursor::arrowCursor().set();
  }
 }
);
impl TaskScrollView {
    fn new(s: &Shared, d: u32, r: NSRect) -> Retained<Self> {
        let m = MainThreadMarker::new().unwrap();
        unsafe {
            msg_send![super(Self::alloc(m).set_ivars(BarIvars{state:Rc::downgrade(s),display:d,tracking:RefCell::new(None)})),initWithFrame:r]
        }
    }
}
fn update_arrow_tracking(view: &NSView, tracking: &RefCell<Option<Retained<NSTrackingArea>>>) {
    // InVisibleRect follows clipping and size changes automatically. Keep the
    // view's one area instead of replacing it whenever AppKit updates tracking.
    if tracking.borrow().is_some() {
        return;
    }
    // SAFETY: both native owners implement mouseEntered: and mouseMoved:.
    // ActiveAlways keeps recovery working in this nonactivating panel; AppKit
    // does not deliver cursorUpdate: for ActiveAlways tracking areas.
    let area = unsafe {
        NSTrackingArea::initWithRect_options_owner_userInfo(
            NSTrackingArea::alloc(),
            view.bounds(),
            NSTrackingAreaOptions::MouseEnteredAndExited
                | NSTrackingAreaOptions::MouseMoved
                | NSTrackingAreaOptions::ActiveAlways
                | NSTrackingAreaOptions::InVisibleRect,
            Some(view),
            None,
        )
    };
    view.addTrackingArea(&area);
    *tracking.borrow_mut() = Some(area);
}
fn contains(r: NSRect, p: NSPoint) -> bool {
    p.x >= r.origin.x
        && p.x < r.origin.x + r.size.width
        && p.y >= r.origin.y
        && p.y < r.origin.y + r.size.height
}
fn reuse_button(
    state: &Shared,
    old: &mut HashMap<Action, Retained<ActionButton>>,
    action: Action,
    title: &str,
    frame: NSRect,
) -> Retained<ActionButton> {
    let b = old
        .remove(&action)
        .unwrap_or_else(|| ActionButton::new(state, action, title, frame));
    if b.frame() != frame {
        b.setFrame(frame);
    }
    if !native_text_matches(&b.title(), &[title]) {
        let title = NSString::from_str(title);
        b.setTitle(&title);
        unsafe {
            let _: () = msg_send![&*b,setAccessibilityLabel:&*title];
        }
    }
    b
}
fn set_children(root: &NSView, children: &[Retained<NSView>]) {
    let existing = root.subviews();
    if existing.len() != children.len()
        || existing
            .iter()
            .zip(children)
            .any(|(a, b)| !std::ptr::eq(&*a, &**b))
    {
        root.setSubviews(&objc2_foundation::NSArray::from_retained_slice(children));
    }
}
fn as_view(b: &Retained<ActionButton>) -> Retained<NSView> {
    b.clone().into_super().into_super().into_super()
}
fn native_text_matches(text: &NSString, parts: &[&str]) -> bool {
    autoreleasepool(|pool| {
        // SAFETY: this is the innermost pool. The retained NSString stays alive,
        // and its borrowed UTF-8 slice never leaves this synchronous comparison.
        let mut remaining = unsafe { text.to_str(pool) };
        for part in parts {
            let Some(next) = remaining.strip_prefix(part) else {
                return false;
            };
            remaining = next;
        }
        remaining.is_empty()
    })
}
fn set_tooltip(button: &NSButton, parts: &[&str]) {
    if !button
        .toolTip()
        .is_some_and(|tooltip| native_text_matches(&tooltip, parts))
    {
        button.setToolTip(Some(&NSString::from_str(&parts.concat())));
    }
}
fn set_app_icon(state: &Shared, button: &NSButton, bundle: &str, path: &str) {
    if let Some(icon) = icon(state, bundle, path)
        && button
            .image()
            .is_none_or(|current| !std::ptr::eq(&*current, &*icon))
    {
        button.setImage(Some(&icon));
    }
}
struct SettingIvars {
    state: WeakState,
    key: String,
}
define_class!(
 #[unsafe(super=NSObject)] #[thread_kind=MainThreadOnly] #[ivars=SettingIvars] #[name = "TaskbarRustSettingTarget"] struct SettingTarget;
 unsafe impl NSObjectProtocol for SettingTarget {}
 impl SettingTarget {
  #[unsafe(method(changed:))] fn changed(&self,sender:&NSControl){if let Some(state)=self.ivars().state.upgrade(){let mut s=state.borrow_mut();number_setting(&mut s.config,&self.ivars().key,sender.doubleValue());if let Err(e)=s.config.save(){s.error=e;}s.dirty=true;}}
 }
);
impl SettingTarget {
    fn new(s: &Shared, key: &str) -> Retained<Self> {
        let m = MainThreadMarker::new().unwrap();
        unsafe {
            msg_send![
                super(Self::alloc(m).set_ivars(SettingIvars {
                    state: Rc::downgrade(s),
                    key: key.into()
                })),
                init
            ]
        }
    }
}
define_class!(
 #[unsafe(super=NSPanel)] #[thread_kind=MainThreadOnly] #[name = "TaskbarRustPopupPanel"] struct PopupPanel;
 unsafe impl NSObjectProtocol for PopupPanel {}
 impl PopupPanel {#[unsafe(method(cancelOperation:))] fn cancel(&self,_sender:Option<&AnyObject>){self.close();}}
);
fn rect(x: f64, y: f64, w: f64, h: f64) -> NSRect {
    NSRect::new(NSPoint::new(x, y), NSSize::new(w, h))
}
fn panel(r: NSRect, title: &str, activating: bool) -> Retained<NSPanel> {
    let m = MainThreadMarker::new().unwrap();
    let style = if activating {
        NSWindowStyleMask::Titled | NSWindowStyleMask::Closable | NSWindowStyleMask::UtilityWindow
    } else {
        NSWindowStyleMask::Borderless | NSWindowStyleMask::NonactivatingPanel
    };
    let p = {
        NSPanel::initWithContentRect_styleMask_backing_defer(
            NSPanel::alloc(m),
            r,
            style,
            NSBackingStoreType::Buffered,
            false,
        )
    };
    unsafe {
        p.setReleasedWhenClosed(false);
        p.setHidesOnDeactivate(false);
        p.setFloatingPanel(true);
    }
    p.setTitle(&NSString::from_str(title));
    p.setLevel(3);
    p.setCollectionBehavior(
        NSWindowCollectionBehavior::CanJoinAllSpaces
            | NSWindowCollectionBehavior::Stationary
            | NSWindowCollectionBehavior::IgnoresCycle
            | NSWindowCollectionBehavior::FullScreenAuxiliary,
    );
    p
}
fn label(t: &str, r: NSRect, font: f64) -> Retained<NSTextField> {
    let t = NSTextField::labelWithString(&NSString::from_str(t), MainThreadMarker::new().unwrap());
    t.setFrame(r);
    t.setFont(Some(&NSFont::systemFontOfSize(font)));
    t
}
fn clear(v: &NSView) {
    for child in v.subviews() {
        child.removeFromSuperview();
    }
}
fn system_dark() -> bool {
    objc2_foundation::NSUserDefaults::standardUserDefaults()
        .stringForKey(objc2_foundation::ns_string!("AppleInterfaceStyle"))
        .is_some_and(|v| v.isEqualToString(objc2_foundation::ns_string!("Dark")))
}
fn set_theme(w: &NSWindow, theme: &str) {
    let name = unsafe {
        match theme {
            "dark" => Some(NSAppearanceNameDarkAqua),
            "light" => Some(NSAppearanceNameAqua),
            _ => None,
        }
    };
    let current = w.appearance();
    let unchanged = match (current.as_deref(), name) {
        (Some(current), Some(name)) => current.name().isEqualToString(name),
        (None, None) => true,
        _ => false,
    };
    if unchanged {
        return;
    }
    w.setAppearance(name.and_then(NSAppearance::appearanceNamed).as_deref());
}
fn screens() -> Vec<(u32, NSRect)> {
    NSScreen::screens(MainThreadMarker::new().unwrap())
        .iter()
        .map(|s| {
            let id = s
                .deviceDescription()
                .objectForKey(objc2_foundation::ns_string!("NSScreenNumber"))
                .map(|o| unsafe { msg_send![&*o, unsignedIntValue] })
                .unwrap_or(0u32);
            (id, s.frame())
        })
        .collect()
}
fn tick(state: &Shared) {
    // AppKit may stay inside nextEvent's run loop while this timer fires for
    // hours. Drain each tick's Foundation temporaries without waiting for a
    // mouse/key event to end the surrounding AppKit autorelease pool.
    autoreleasepool(|_| tick_inner(state));
}
fn apply_snapshot(s: &mut State, snapshot: Snapshot) -> bool {
    if s.pending_tab.is_some_and(|(id, index, at)| {
        at.elapsed() >= Duration::from_secs(2)
            || snapshot.control_error.is_some()
            || !snapshot.trusted
            || snapshot
                .windows
                .iter()
                .find(|w| w.id == id)
                .is_none_or(|w| {
                    w.tabs
                        .iter()
                        .find(|t| t.id == index)
                        .is_none_or(|t| t.selected)
                })
    }) {
        s.pending_tab = None;
        s.dirty = true;
    }
    if s.pending_focus.is_some_and(|pending| {
        pending.resolved(
            snapshot.windows.iter().find(|w| w.focused).map(|w| w.id),
            snapshot.windows.iter().any(|w| w.id == pending.id),
            snapshot.trusted,
            snapshot.control_error.is_some(),
            Instant::now(),
        )
    }) {
        s.pending_focus = None;
        s.dirty = true;
    }
    if let Some(error) = &snapshot.control_error {
        s.error = error.clone();
    }
    let permissions_changed = snapshot.trusted != s.snapshot.trusted
        || snapshot.screen_allowed != s.snapshot.screen_allowed;
    let windows_changed = snapshot.windows != s.snapshot.windows;
    if windows_changed || snapshot.badges != s.snapshot.badges || permissions_changed {
        s.dirty = true;
    }
    // Diagnostics and pending focus still update on an unchanged window list.
    // Disabled previews must release their image, hover, cache, and capture first.
    let preview_cleanup = (!s.config.thumbnails || !snapshot.screen_allowed)
        && (s.hover.is_some()
            || s.preview_id.is_some()
            || s.capture_busy
            || s.preview_cache.len() != 0);
    if !windows_changed && !permissions_changed && !preview_cleanup {
        s.snapshot = snapshot;
        return false;
    }
    let changed = snapshot.windows.iter().find(|w| w.focused).map(|w| w.id)
        != s.snapshot.windows.iter().find(|w| w.focused).map(|w| w.id);
    if changed {
        s.hidden_now.clear();
    }
    s.activity.observe(&snapshot.windows);
    let alive: HashSet<u32> = snapshot.windows.iter().map(|w| w.id).collect();
    let (hover_invalid, preview_invalid) = preview_lifecycle::invalid_targets(
        s.hover.map(|h| h.0),
        s.preview_id,
        &alive,
        s.config.thumbnails && snapshot.screen_allowed,
    );
    if s.capture_busy
        && (hover_invalid
            || preview_lifecycle::capture_invalid(
                s.capture_source,
                &snapshot.windows,
                s.config.thumbnails && snapshot.screen_allowed,
            ))
    {
        crate::capture::cancel_pending();
    }
    if hover_invalid {
        s.hover = None;
    }
    // A popup can outlive mouseExited while its pointer is inside it.
    // Closing its source must hide/release it even with no hover target.
    if hover_invalid || preview_invalid {
        s.preview_id = None;
        s.preview_until = None;
        if let Some(p) = &s.preview {
            p.panel.orderOut(None);
            p.image.setImage(None);
        }
    }
    crate::window_order::reconcile(
        &mut s.order,
        &mut s.known_spaces,
        &s.snapshot.windows,
        &snapshot.windows,
        s.config.reset_space_order,
        &alive,
    );
    s.snapshot = snapshot;
    prune_preview_sources(s, &alive);
    permissions_changed
}
fn tick_inner(state: &Shared) {
    let mut permissions_changed = false;
    let dark = system_dark();
    if state.borrow().observed_dark != dark {
        let mut s = state.borrow_mut();
        s.observed_dark = dark;
        s.dirty = true;
    }
    {
        let mut s = state.borrow_mut();
        let queue_error = s.queue_error.borrow_mut().take();
        if let Some(error) = queue_error {
            s.error = error;
        }
        while let Ok(snapshot) = s.rx.try_recv() {
            s.received_snapshot = true;
            permissions_changed |= apply_snapshot(&mut s, snapshot);
        }
        if s.gui_smoke.is_none() {
            let enabled = s.config.chrome_profile_badges;
            let State {
                profile_badges,
                snapshot,
                ..
            } = &mut *s;
            if profile_badges.poll(&snapshot.windows, enabled) {
                s.dirty = true;
            }
        }
        if s.pending_focus
            .is_some_and(|pending| pending.expired(Instant::now()))
        {
            s.pending_focus = None;
            s.dirty = true;
        }
        if s.pending_tab
            .is_some_and(|(_, _, at)| at.elapsed() >= Duration::from_secs(2))
        {
            s.pending_tab = None;
            s.dirty = true;
        }
        if let Ok(apps) = s.apps_rx.try_recv() {
            s.apps = apps;
            s.catalog_busy = false;
            if let Some(m) = s.start.as_mut() {
                m.query = "\0".into();
            }
            s.dirty = true;
        }
        poll_images(&mut s);
        if s.start.as_ref().is_some_and(|m| !m.panel.isVisible())
            && let Some(m) = s.start.take()
        {
            m.panel.close();
            s.dirty = true;
        }
        if s.gui_smoke.is_none() {
            release_closed_preferences(&mut s);
        }
    }
    if state.borrow().pending_system_sort && state.borrow().received_snapshot {
        state.borrow_mut().pending_system_sort = false;
        dispatch(state, Action::Sort);
    }
    if permissions_changed {
        refresh_permission_labels(state);
        if state.borrow().snapshot.trusted {
            hover_close::install(state);
        } else {
            state.borrow_mut().hover_close.take();
        }
    }
    let render_started = Instant::now();
    let frames = screens();
    if state.borrow().last_screens != frames {
        state.borrow_mut().dirty = true;
    }
    if state.borrow().dirty {
        render_with_frames(state, frames, render_started);
    }
    if state.borrow().gui_smoke.is_none() {
        update_hover(state);
    }
    prefetch_preview(state);
    update_start_search(state);
    if gui_smoke_tick(state) {
        return;
    }
    let update = {
        let mut s = state.borrow_mut();
        let State {
            config, updater, ..
        } = &mut *s;
        updater.policy(&config.update_policy);
        updater.poll()
    };
    if let Some(crate::updater::Event::Checked(Err(error))) = update {
        state.borrow_mut().error = error;
    }
    if state.borrow().last_metrics.elapsed() > Duration::from_secs(10) {
        persist_metrics(&state.borrow());
        state.borrow_mut().last_metrics = Instant::now();
    }
}
fn gui_smoke_tick(state: &Shared) -> bool {
    if state.borrow().hover_benchmark.is_some() {
        qa::hover_tick(state);
        return true;
    }
    let benchmark = state.borrow_mut().ui_benchmark.take();
    if let Some(count) = benchmark {
        if !qa::run(state, count) {
            state.borrow_mut().error = "Native hit/handler benchmark failed".into();
        }
        NSApplication::sharedApplication(MainThreadMarker::new().unwrap()).terminate(None);
        return true;
    }
    let step = {
        let mut s = state.borrow_mut();
        let Some(test) = s.gui_smoke.as_mut() else {
            return false;
        };
        if test.last.elapsed() < Duration::from_millis(200) {
            return false;
        }
        test.last = Instant::now();
        let step = test.total - test.remaining;
        if test.remaining > 0 {
            test.remaining -= 1;
        } else {
            println!(
                "{}",
                serde_json::json!({"gui_smoke_cycles":test.total,"completed":true,"windows":s.snapshot.windows.len(),"icons":s.icons.len(),"accessibility":s.snapshot.trusted,"screen_recording":s.snapshot.screen_allowed,"discovery":s.snapshot.discovery})
            );
            drop(s);
            NSApplication::sharedApplication(MainThreadMarker::new().unwrap()).terminate(None);
            return true;
        }
        step
    };
    build_preferences(state, step % 10);
    let _menu = context_menu(state, None);
    if step % 10 == 0 {
        let display = state.borrow().bars.first().map(|b| b.display).unwrap_or(0);
        build_start(state, display);
    }
    if let Some(menu) = state.borrow_mut().start.take() {
        menu.panel.close();
    }
    false
}
// Only these preferences cross native callbacks during a bar redraw. Unknown
// settings, application history and picker settings stay in their owning Config.
struct BarSettings {
    pins: Vec<crate::config::Pin>,
    blacklist: Vec<String>,
    hidden_displays: Vec<u32>,
    theme: String,
    show_tabs: bool,
    compact_related_windows: bool,
    show_hidden: bool,
    discord_hidden: bool,
    all_displays: bool,
    group_by_app: bool,
    main_only: bool,
    show_sort: bool,
    show_start: bool,
    show_titles: bool,
    center: bool,
    indicate_minimized: bool,
    indicate_hidden: bool,
    show_all_spaces: bool,
    resize_overlap: bool,
    scale: f64,
    font_size: f64,
    transparency: f64,
    max_width: f64,
}
impl From<&Config> for BarSettings {
    fn from(c: &Config) -> Self {
        Self {
            pins: c.pins.clone(),
            blacklist: c.blacklist.clone(),
            hidden_displays: c.hidden_displays.clone(),
            theme: c.theme.clone(),
            show_tabs: c.show_tabs,
            compact_related_windows: c.compact_related_windows,
            show_hidden: c.show_hidden,
            discord_hidden: c.discord_hidden,
            all_displays: c.all_displays,
            group_by_app: c.group_by_app,
            main_only: c.main_only,
            show_sort: c.show_sort,
            show_start: c.show_start,
            show_titles: c.show_titles,
            center: c.center,
            indicate_minimized: c.indicate_minimized,
            indicate_hidden: c.indicate_hidden,
            show_all_spaces: c.show_all_spaces,
            resize_overlap: c.resize_overlap,
            scale: c.scale,
            font_size: c.font_size,
            transparency: c.transparency,
            max_width: c.max_width,
        }
    }
}
fn render(state: &Shared) {
    let render_started = Instant::now();
    render_with_frames(state, screens(), render_started);
}
fn render_with_frames(state: &Shared, frames: Vec<(u32, NSRect)>, render_started: Instant) {
    {
        let mut s = state.borrow_mut();
        if s.last_screens != frames {
            for b in &s.bars {
                b.panel.close();
            }
            s.bars.clear();
            s.last_screens = frames.clone();
            for (d, f) in &frames {
                let h = 32.0 * s.config.scale / 100.0;
                let p = panel(
                    rect(f.origin.x, f.origin.y, f.size.width, h),
                    "Rowla",
                    false,
                );
                p.setOpaque(false);
                p.setHasShadow(false);
                p.setAcceptsMouseMovedEvents(true);
                let root = BarView::new(state, *d, rect(0.0, 0.0, f.size.width, h));
                p.setContentView(Some(&root));
                let scroll = TaskScrollView::new(state, *d, rect(0.0, 0.0, f.size.width, h));
                scroll.setDrawsBackground(false);
                scroll.setBorderType(NSBorderType::NoBorder);
                scroll.setHasHorizontalScroller(false);
                scroll.setHasVerticalScroller(false);
                scroll.setHorizontalScrollElasticity(NSScrollElasticity::None);
                scroll.setVerticalScrollElasticity(NSScrollElasticity::None);
                scroll
                    .contentView()
                    .setDocumentCursor(Some(&NSCursor::arrowCursor()));
                let tasks = NSView::initWithFrame(
                    NSView::alloc(MainThreadMarker::new().unwrap()),
                    rect(0.0, 0.0, f.size.width, h),
                );
                scroll.setDocumentView(Some(&tasks));
                s.bars.push(Bar {
                    display: *d,
                    frame: *f,
                    panel: p,
                    root,
                    tasks,
                    scroll,
                    buttons: HashMap::new(),
                });
            }
        }
        if s.config.show_menubar && s.status.is_none() {
            let item = NSStatusBar::systemStatusBar().statusItemWithLength(-1.0);
            if let Some(b) = item.button(MainThreadMarker::new().unwrap()) {
                b.setImage(Some(&brand::status_icon()));
                b.setToolTip(Some(&NSString::from_str("Rowla — Preferences")));
                // SAFETY: NSStatusBarButton supports the NSAccessibility label selector.
                unsafe {
                    let _: () =
                        msg_send![&*b, setAccessibilityLabel: &*NSString::from_str("Rowla")];
                }
                unsafe {
                    b.setTarget(Some(s.delegate()));
                    b.setAction(Some(sel!(statusAction:)));
                }
            }
            s.status = Some(item);
        } else if !s.config.show_menubar
            && let Some(i) = s.status.take()
        {
            NSStatusBar::systemStatusBar().removeStatusItem(&i);
        }
        s.dirty = false;
    }
    let (c, windows, positions, focused_id) = {
        let s = state.borrow();
        (
            BarSettings::from(&s.config),
            s.snapshot.windows.clone(),
            s.order
                .iter()
                .enumerate()
                .map(|(i, id)| (*id, i))
                .collect::<HashMap<_, _>>(),
            s.focused_id(),
        )
    };
    let mut open_apps: HashSet<&str> = windows.iter().map(|w| w.bundle.as_str()).collect();
    let pins: Vec<_> = c
        .pins
        .iter()
        .filter(|pin| !open_apps.contains(pin.bundle.as_str()))
        .collect();
    let mut eligible: Vec<_> = windows
        .iter()
        .filter(|w| {
            w.on_space
                && (c.compact_related_windows || c.show_tabs || !w.tabbed_hidden)
                && !c.blacklist.contains(&w.bundle)
                && (c.show_hidden || !w.hidden)
                && !(c.discord_hidden && w.bundle == "com.hnc.Discord" && w.hidden)
        })
        .collect();
    eligible.sort_by_cached_key(|w| positions.get(&w.id).copied().unwrap_or(usize::MAX));
    let main_id = frames.first().map(|s| s.0).unwrap_or(0);
    let top = frames
        .first()
        .map(|s| s.1.origin.y + s.1.size.height)
        .unwrap_or(982.0);
    let background = if c.theme == "light" || (c.theme == "auto" && !system_dark()) {
        NSColor::colorWithSRGBRed_green_blue_alpha(0.92, 0.93, 0.95, 1.0 - c.transparency / 100.0)
    } else {
        NSColor::colorWithSRGBRed_green_blue_alpha(0.12, 0.13, 0.16, 1.0 - c.transparency / 100.0)
    };
    let count = state.borrow().bars.len();
    for i in 0..count {
        let (d, f, p, root, task_root, scroll, mut old) = {
            let mut s = state.borrow_mut();
            let b = &mut s.bars[i];
            (
                b.display,
                b.frame,
                b.panel.clone(),
                b.root.clone(),
                b.tasks.clone(),
                b.scroll.clone(),
                std::mem::take(&mut b.buttons),
            )
        };
        let h = 32.0 * c.scale / 100.0;
        let frame = rect(f.origin.x, f.origin.y, f.size.width, h);
        if p.frame() != frame {
            p.setFrame_display(frame, true);
        }
        if root.frame().size != frame.size {
            root.setFrameSize(frame.size);
        }
        set_theme(&p, &c.theme);
        if p.backgroundColor() != background {
            p.setBackgroundColor(Some(&background));
        }
        let mut tasks: Vec<&Window> = eligible
            .iter()
            .copied()
            .filter(|w| c.all_displays || window_on_display(w, f, top))
            .collect();
        if c.group_by_app {
            let mut apps = HashMap::new();
            for w in &tasks {
                let next = apps.len();
                apps.entry(w.bundle.as_str()).or_insert(next);
            }
            tasks.sort_by_cached_key(|w| apps[w.bundle.as_str()]);
        }
        let groups = if c.compact_related_windows {
            crate::related_windows::groups(&tasks)
        } else {
            tasks
                .iter()
                .map(|w| crate::related_windows::Group {
                    main: w,
                    children: Vec::new(),
                })
                .collect()
        };
        let groups: Vec<_> = groups
            .into_iter()
            .filter(|g| c.show_tabs || !g.main.tabbed_hidden)
            .collect();
        let hidden = {
            let s = state.borrow();
            s.hidden_now.contains(&d)
                || c.hidden_displays.contains(&d)
                || (c.main_only && d != main_id)
                || tasks
                    .iter()
                    .any(|w| w.focused && w.fullscreen && window_on_display(w, f, top))
        };
        if hidden {
            p.orderOut(None);
            state.borrow_mut().bars[i].buttons = old;
            continue;
        }
        // Reserve once for this redraw. Hidden bars keep their existing map.
        let mut buttons = HashMap::with_capacity(tasks.len() + pins.len() + 5);
        let mut fixed_children = Vec::with_capacity(pins.len() + 6);
        let mut task_children = Vec::with_capacity(tasks.len());
        let iw = h + 4.0;
        let sort_width = if c.show_sort { iw } else { 0.0 };
        let fixed = (pins.len() + usize::from(c.show_start)) as f64 * iw + sort_width;
        let has_bubbles = c.compact_related_windows
            && groups
                .iter()
                .any(|g| !g.children.is_empty() || g.main.tab_count > 1);
        let tw = if c.show_titles {
            ((f.size.width - fixed - 8.0) / groups.len().max(1) as f64).clamp(
                if has_bubbles {
                    54.0 * c.scale / 100.0
                } else {
                    48.0
                },
                c.max_width * c.scale / 100.0,
            )
        } else if has_bubbles {
            iw + 24.0 * c.scale / 100.0
        } else {
            iw
        };
        let mut x = if c.center {
            ((f.size.width - fixed - tw * groups.len() as f64) / 2.0).max(0.0)
        } else {
            0.0
        };
        if c.show_start {
            let b = reuse_button(state, &mut old, Action::Start(d), "▦", rect(x, 0.0, iw, h));
            set_tooltip(&b, &["Start — installed applications"]);
            fixed_children.push(as_view(&b));
            buttons.insert(b.ivars().action.clone(), b);
            x += iw;
        }
        if c.show_sort {
            let b = reuse_button(
                state,
                &mut old,
                Action::Sort,
                "",
                rect(x, 0.0, sort_width, h),
            );
            b.setImagePosition(NSCellImagePosition::ImageOnly);
            b.setAlignment(NSTextAlignment::Center);
            set_tooltip(
                &b,
                &[
                    "Sort windows — restore application order; recent main windows first, popups last. Right-click to edit the order.",
                ],
            );
            fixed_children.push(as_view(&b));
            buttons.insert(Action::Sort, b);
            x += sort_width;
        }
        for pin in &pins {
            let b = reuse_button(
                state,
                &mut old,
                Action::Pin(pin.bundle.clone(), pin.action.clone()),
                "",
                rect(x, 0.0, iw, h),
            );
            set_app_icon(state, &b, &pin.bundle, "");
            set_tooltip(&b, &[&pin.bundle]);
            set_badge(state, &b, &pin.bundle);
            fixed_children.push(as_view(&b));
            buttons.insert(b.ivars().action.clone(), b);
            x += iw;
        }
        if !state.borrow().snapshot.trusted {
            let b = reuse_button(
                state,
                &mut old,
                Action::Preferences(6),
                "Grant Accessibility to list and switch windows",
                rect(x + 4.0, 0.0, 370.0, h),
            );
            fixed_children.push(as_view(&b));
            buttons.insert(b.ivars().action.clone(), b);
        }
        let available = (f.size.width - x).max(1.0);
        let overflow = tw * groups.len() as f64 > available;
        let arrow_width = if overflow {
            h.min(available / 3.0)
        } else {
            0.0
        };
        let viewport = rect(
            x + arrow_width,
            0.0,
            (available - arrow_width * 2.0).max(1.0),
            h,
        );
        if scroll.frame() != viewport {
            scroll.setFrame(viewport);
        }
        let task_size = NSSize::new((tw * groups.len() as f64).max(viewport.size.width), h);
        if task_root.frame().size != task_size {
            task_root.setFrameSize(task_size);
        }
        let clip = scroll.contentView();
        let offset = clip
            .bounds()
            .origin
            .x
            .clamp(0.0, (task_size.width - viewport.size.width).max(0.0));
        if clip.bounds().origin.x != offset {
            clip.scrollToPoint(NSPoint::new(offset, 0.0));
            scroll.reflectScrolledClipView(&clip);
        }
        if overflow {
            for (right, at, title) in [(false, x, "‹"), (true, f.size.width - arrow_width, "›")]
            {
                let b = reuse_button(
                    state,
                    &mut old,
                    Action::ScrollTasks(d, right),
                    title,
                    rect(at, 0.0, arrow_width, h),
                );
                unsafe {
                    let _: () = msg_send![&*b,setAccessibilityLabel:&*NSString::from_str(if right{"Show later windows"}else{"Show earlier windows"})];
                }
                fixed_children.push(as_view(&b));
                buttons.insert(b.ivars().action.clone(), b);
            }
        }
        fixed_children.push(scroll.clone().into_super().into_super());
        x = 0.0;
        for group in groups {
            let w = group.main;
            let title = if !c.show_titles {
                std::borrow::Cow::Borrowed("")
            } else {
                let title = if w.title.is_empty() {
                    w.app.as_str()
                } else {
                    w.title.as_str()
                };
                let (left, right) = if w.minimized && c.indicate_minimized {
                    ("[", "]")
                } else if w.hidden && c.indicate_hidden {
                    ("(", ")")
                } else {
                    ("", "")
                };
                let prefix = if w.all_spaces && c.show_all_spaces {
                    "⌑ "
                } else {
                    ""
                };
                if left.is_empty() && prefix.is_empty() {
                    std::borrow::Cow::Borrowed(title)
                } else {
                    std::borrow::Cow::Owned(format!("{prefix}{left}{title}{right}"))
                }
            };
            let new = !old.contains_key(&Action::Window(w.id));
            let b = reuse_button(
                state,
                &mut old,
                Action::Window(w.id),
                &title,
                rect(x, 0.0, tw, h),
            );
            b.set_window_focus(
                focused_id == Some(w.id) || group.children.iter().any(|w| focused_id == Some(w.id)),
                c.font_size,
            );
            let mut ids = b.ivars().related_ids.borrow_mut();
            if !ids.iter().copied().eq(group.children.iter().map(|w| w.id)) {
                ids.clear();
                ids.extend(group.children.iter().map(|w| w.id));
            }
            drop(ids);
            let position = if c.show_titles {
                NSCellImagePosition::ImageLeft
            } else {
                NSCellImagePosition::ImageOnly
            };
            if b.imagePosition() != position {
                b.setImagePosition(position);
            }
            set_app_icon(state, &b, &w.bundle, &w.path);
            chrome_profiles::apply(state, &b, w);
            let alpha = if w.minimized || w.hidden { 0.65 } else { 1.0 };
            if b.alphaValue() != alpha {
                b.setAlphaValue(alpha);
            }
            if new {
                let tracking = unsafe {
                    NSTrackingArea::initWithRect_options_owner_userInfo(
                        NSTrackingArea::alloc(),
                        rect(0.0, 0.0, tw, h),
                        NSTrackingAreaOptions::MouseEnteredAndExited
                            | NSTrackingAreaOptions::MouseMoved
                            | NSTrackingAreaOptions::ActiveAlways
                            | NSTrackingAreaOptions::InVisibleRect,
                        Some(&b),
                        None,
                    )
                };
                {
                    b.addTrackingArea(&tracking);
                }
            }
            set_badge(state, &b, &w.bundle);
            task_children.push(as_view(&b));
            let reserved = if c.compact_related_windows {
                related::render_bubbles(
                    state,
                    &group,
                    rect(x, 0.0, tw, h),
                    focused_id,
                    &mut old,
                    &mut buttons,
                    &mut task_children,
                )
            } else {
                0.0
            };
            if b.ivars().reserved_width.replace(reserved) != reserved {
                NSView::setNeedsDisplay(&b, true);
            }
            buttons.insert(b.ivars().action.clone(), b);
            x += tw;
            if state.borrow().gui_smoke.is_none()
                && c.resize_overlap
                && !w.minimized
                && !w.hidden
                && !w.fullscreen
                && window_on_display(w, f, top)
            {
                let limit = top - f.origin.y - h;
                if w.y + w.height > limit + 1.0 {
                    state.borrow().command(Command::Resize(w.id, limit));
                }
            }
        }
        set_children(&task_root, &task_children);
        set_children(&root, &fixed_children);
        state.borrow_mut().bars[i].buttons = buttons;
        if state.borrow().gui_smoke.is_none() && !p.isVisible() {
            p.orderFrontRegardless();
        }
    }
    let mut s = state.borrow_mut();
    if s.start.is_none() {
        open_apps.extend(c.pins.iter().map(|p| p.bundle.as_str()));
        s.icons.retain(|k, _| open_apps.contains(k.as_str()));
    }
    s.performance.render.record(render_started.elapsed());
}
fn set_badge(state: &Shared, button: &ActionButton, bundle: &str) {
    let s = state.borrow();
    let visible =
        s.config.show_badges && s.snapshot.badges.get(bundle).is_some_and(|v| !v.is_empty());
    if button.ivars().badge.replace(visible) != visible {
        NSView::setNeedsDisplay(button, true);
    }
}
fn window_on_display(w: &Window, f: NSRect, top: f64) -> bool {
    let x = w.x + w.width / 2.0;
    let y = top - w.y - w.height / 2.0;
    x >= f.origin.x
        && x < f.origin.x + f.size.width
        && y >= f.origin.y
        && y < f.origin.y + f.size.height
}
fn icon(state: &Shared, bundle: &str, path: &str) -> Option<Retained<NSImage>> {
    if let Some(i) = state.borrow().icons.get(bundle) {
        return Some(i.clone());
    }
    let ws = NSWorkspace::sharedWorkspace();
    let p = if path.is_empty() {
        ws.URLForApplicationWithBundleIdentifier(&NSString::from_str(bundle))
            .and_then(|u| u.path())
            .map(|p| p.to_string())
            .unwrap_or_default()
    } else {
        path.into()
    };
    if p.is_empty() {
        return None;
    }
    let image = ws.iconForFile(&NSString::from_str(&p));
    image.setSize(NSSize::new(24.0, 24.0));
    state
        .borrow_mut()
        .icons
        .insert(bundle.into(), image.clone());
    Some(image)
}
fn activate_window(state: &Shared, id: u32) {
    let started = Instant::now();
    let buttons = {
        let mut s = state.borrow_mut();
        if !s.snapshot.windows.iter().any(|w| w.id == id)
            || !s.command(Command::Activate(id, false))
        {
            return;
        }
        s.pending_tab = None;
        let previous = s.focused_id();
        s.pending_focus = Some(active_feedback::PendingFocus::new(id, started));
        s.bars
            .iter()
            .flat_map(|bar| bar.buttons.values())
            .filter_map(|button| match button.ivars().action {
                Action::Window(window) | Action::Bubble(window) => {
                    let related = button.ivars().related_ids.borrow();
                    let focused = window == id || related.contains(&id);
                    let was_focused = Some(window) == previous
                        || previous.is_some_and(|id| related.contains(&id));
                    (focused || was_focused).then(|| {
                        (
                            button.clone(),
                            focused,
                            if matches!(button.ivars().action, Action::Bubble(_)) {
                                9.0 * s.config.scale / 100.0
                            } else {
                                s.config.font_size
                            },
                        )
                    })
                }
                _ => None,
            })
            .collect::<Vec<_>>()
    };
    for (button, focused, size) in buttons {
        button.set_window_focus(focused, size);
        button.displayIfNeeded();
    }
    state
        .borrow_mut()
        .performance
        .click_feedback
        .record(started.elapsed());
}
fn system_action(state: &Shared, action: crate::system_actions::SystemAction) {
    match action {
        crate::system_actions::SystemAction::Sort => state.borrow_mut().pending_system_sort = true,
        crate::system_actions::SystemAction::Show => dispatch(state, Action::ShowBars),
        crate::system_actions::SystemAction::Hide => dispatch(state, Action::HideBars),
    }
}
fn dispatch(state: &Shared, a: Action) {
    let preview_id = state.borrow().preview_id;
    hide_preview(state);
    match a {
        Action::Sort => sort_order::apply(state),
        Action::OpenShortcuts => {
            state.borrow().command(Command::Launch(
                "com.apple.shortcuts".into(),
                "launchOrActivateApp".into(),
            ));
        }
        Action::InstallShortcut(action) => {
            if let Some(resources) = objc2_foundation::NSBundle::mainBundle().resourceURL() {
                let path = resources.URLByAppendingPathComponent(&NSString::from_str(&format!(
                    "shortcuts/{}.shortcut",
                    action.shortcut_name()
                )));
                if let Some(path) = path {
                    NSWorkspace::sharedWorkspace().openURL(&path);
                }
            }
        }
        Action::AddOrderApp => {
            let display = state.borrow().bars.first().map(|b| b.display).unwrap_or(0);
            if let Some(menu) = state.borrow_mut().start.take() {
                menu.panel.close();
            }
            build_start(state, display);
            if let Some(menu) = state.borrow_mut().start.as_mut() {
                menu.purpose = PickerPurpose::AppOrder;
                menu.query = "\0".into();
                menu.panel
                    .setTitle(&NSString::from_str("Add application to sort order"));
            }
            search_start(state);
        }
        Action::AppendOrderApp(bundle) => sort_order::edit(state, sort_order::Edit::Add(bundle)),
        Action::MoveOrderApp(bundle, down) => {
            sort_order::edit(state, sort_order::Edit::Move(bundle, down))
        }
        Action::RemoveOrderApp(bundle) => sort_order::edit(state, sort_order::Edit::Remove(bundle)),
        Action::UseTaskOrder => sort_order::edit(state, sort_order::Edit::Tasks),
        Action::UseDockOrder => sort_order::edit(state, sort_order::Edit::Dock),
        Action::ScrollTasks(display, right) => {
            let s = state.borrow();
            if let Some(b) = s.bars.iter().find(|b| b.display == display) {
                let clip = b.scroll.contentView();
                let width = clip.bounds().size.width;
                let offset = (clip.bounds().origin.x
                    + if right { width * 0.8 } else { -width * 0.8 })
                .clamp(0.0, (b.tasks.frame().size.width - width).max(0.0));
                clip.scrollToPoint(NSPoint::new(offset, 0.0));
                b.scroll.reflectScrolledClipView(&clip);
            }
        }
        Action::AddBlacklist => {
            let display = state.borrow().bars.first().map(|b| b.display).unwrap_or(0);
            build_start(state, display);
            if let Some(m) = state.borrow_mut().start.as_mut() {
                m.purpose = PickerPurpose::Blacklist;
                m.query = "\0".into();
                m.panel
                    .setTitle(&NSString::from_str("Choose an application to blacklist"));
            }
            search_start(state);
        }
        Action::CheckUpdates => {
            if state.borrow_mut().updater.check() {
                state.borrow_mut().error = "Checking for updates with Sparkle…".into();
            }
            show_preferences(state, 7);
        }
        Action::ClosePreview => {
            if let Some(id) = preview_id {
                state.borrow().command(Command::Close(id));
            }
        }
        Action::RelatedMore(id) => related::show_more(state, id),
        Action::Tab(id, index) => related::select_tab(state, id, index),
        Action::CloseTab(id, index) => {
            let s = state.borrow();
            if let Some(tab) = s
                .snapshot
                .windows
                .iter()
                .find(|w| w.id == id)
                .and_then(|w| w.tabs.iter().find(|t| t.id == index))
            {
                s.command(Command::CloseTab(tab.id));
            }
        }
        Action::Raise(id) | Action::Bubble(id) => {
            activate_window(state, id);
        }
        Action::Preview(id) => {
            let anchor = {
                let s = state.borrow();
                s.bars.iter().find_map(|b| {
                    b.buttons
                        .values()
                        .find(|v| matches!(v.ivars().action,Action::Window(w)|Action::Bubble(w) if w==id))
                        .map(|v| {
                            b.panel
                                .convertRectToScreen(v.convertRect_toView(v.bounds(), None))
                        })
                })
            };
            if let Some(r) = anchor {
                begin_hover(state, id, 0, r);
                let mut s = state.borrow_mut();
                s.hover = Some((id, 0, Instant::now() - Duration::from_secs(5)));
                s.preview_until = Some(Instant::now() + Duration::from_secs(15));
            }
        }
        Action::Window(id) => {
            let cmd = {
                let s = state.borrow();
                s.snapshot.windows.iter().find(|w| w.id == id).map(|w| {
                    // Discovery can still report the previous window as focused
                    // after a click. Pending, hidden or minimized targets must
                    // activate rather than toggle from that stale report.
                    if w.focused && s.pending_focus.is_none() && !w.hidden && !w.minimized {
                        if s.config.click_hides_app {
                            Command::Hide(w.pid)
                        } else {
                            Command::Minimize(id)
                        }
                    } else {
                        Command::Activate(id, false)
                    }
                })
            };
            if let Some(Command::Activate(id, false)) = cmd {
                activate_window(state, id);
            } else if let Some(c) = cmd {
                state.borrow_mut().pending_focus = None;
                state.borrow_mut().dirty = true;
                state.borrow().command(c);
            }
        }
        Action::Pin(b, a) => {
            state.borrow().command(Command::Launch(b.clone(), a));
            let mut s = state.borrow_mut();
            s.config.remember(&b);
            if let Err(e) = s.config.save() {
                s.error = e;
            }
            if let Some(m) = s.start.take() {
                m.panel.close();
            }
        }
        Action::Start(d) => {
            if state.borrow().config.start_action == "spotlight" {
                state.borrow().command(Command::Launch(
                    "com.apple.apps.launcher".into(),
                    "launchOrActivateApp".into(),
                ));
            } else {
                show_start(state, d);
            }
        }
        Action::Preferences(p) => show_preferences(state, p),
        Action::Minimize(id) => {
            state.borrow().command(Command::Minimize(id));
        }
        Action::Close(id) => {
            state.borrow().command(Command::Close(id));
        }
        Action::Fullscreen(id) => {
            state.borrow().command(Command::Fullscreen(id));
        }
        Action::HideApp(pid) => {
            state.borrow().command(Command::Hide(pid));
        }
        Action::QuitApp(pid) => {
            state.borrow().command(Command::Quit(pid));
        }
        Action::Keep(b) => {
            let mut s = state.borrow_mut();
            if !s.config.pins.iter().any(|p| p.bundle == b) && s.config.pins.len() < 64 {
                s.config.pins.push(Pin {
                    bundle: b,
                    action: "launchOrActivateApp".into(),
                });
            }
            if let Err(e) = s.config.save() {
                s.error = e;
            }
            s.dirty = true;
        }
        Action::Unpin(b) => {
            let mut s = state.borrow_mut();
            s.config.pins.retain(|p| p.bundle != b);
            if let Err(e) = s.config.save() {
                s.error = e;
            }
            s.dirty = true;
        }
        Action::Blacklist(b) => {
            let mut s = state.borrow_mut();
            if !s.config.blacklist.contains(&b) {
                s.config.blacklist.push(b);
            }
            if let Err(e) = s.config.save() {
                s.error = e;
            }
            s.dirty = true;
            let picker = s
                .start
                .as_ref()
                .is_some_and(|m| m.purpose == PickerPurpose::Blacklist);
            if picker && let Some(m) = s.start.take() {
                m.panel.close();
            }
            drop(s);
            if picker {
                show_preferences(state, 4);
            }
        }
        Action::RemoveBlacklist(b) => {
            let mut s = state.borrow_mut();
            s.config.blacklist.retain(|p| p != &b);
            if let Err(e) = s.config.save() {
                s.error = e;
            }
            s.dirty = true;
            drop(s);
            show_preferences(state, 4);
        }
        Action::PinBehavior(b, a) => {
            let mut s = state.borrow_mut();
            if let Some(p) = s.config.pins.iter_mut().find(|p| p.bundle == b) {
                p.action = a;
            }
            if let Err(e) = s.config.save() {
                s.error = e;
            }
        }
        Action::HideBar(d, now) => {
            let mut s = state.borrow_mut();
            if now {
                s.hidden_now.insert(d);
            } else {
                if !s.config.hidden_displays.contains(&d) {
                    s.config.hidden_displays.push(d);
                }
                if let Err(e) = s.config.save() {
                    s.error = e;
                }
            }
            s.dirty = true;
        }
        Action::ShowBars => {
            let mut s = state.borrow_mut();
            s.hidden_now.clear();
            s.config.hidden_displays.clear();
            if s.gui_smoke.is_none()
                && let Err(e) = s.config.save()
            {
                s.error = e;
            }
            s.dirty = true;
        }
        Action::HideBars => {
            let mut s = state.borrow_mut();
            s.config.hidden_displays = screens().iter().map(|(id, _)| *id).collect();
            // Keep a restoration control even if the user previously hid the icon.
            s.config.show_menubar = true;
            if s.gui_smoke.is_none()
                && let Err(error) = s.config.save()
            {
                s.error = error;
            }
            s.dirty = true;
        }
        Action::Toggle(k) => {
            let mut s = state.borrow_mut();
            if let Some(v) = bool_setting(&mut s.config, &k) {
                *v = !*v;
            }
            if k == "fully_hide_dock" {
                let _ = s.tx.try_send(Command::Dock(s.config.fully_hide_dock));
            }
            if k == "start_at_login"
                && let Err(e) = platform::login(s.config.start_at_login)
            {
                s.error = e;
            }
            if let Err(e) = s.config.save() {
                s.error = e;
            }
            s.dirty = true;
            let page = s.preferences.as_ref().map(|p| p.page);
            drop(s);
            if let Some(p) = page {
                show_preferences(state, p);
            }
        }
        Action::Cycle(k) => {
            let mut s = state.borrow_mut();
            if k == "theme" {
                s.config.theme = match s.config.theme.as_str() {
                    "auto" => "light",
                    "light" => "dark",
                    _ => "auto",
                }
                .into();
            } else if k == "start_action" {
                s.config.start_action = if s.config.start_action == "menu" {
                    "spotlight"
                } else {
                    "menu"
                }
                .into();
            } else if k == "capture_mode" {
                s.config.capture_mode = match s.config.capture_mode {
                    crate::config::CaptureMode::Stream => crate::config::CaptureMode::Snapshot,
                    crate::config::CaptureMode::Snapshot => crate::config::CaptureMode::Stream,
                };
                crate::capture::cancel_pending();
            } else if k == "update_policy" {
                s.config.update_policy = match s.config.update_policy.as_str() {
                    "manual" => "check",
                    "check" => "automatic",
                    _ => "manual",
                }
                .into();
            }
            if s.gui_smoke.is_none()
                && let Err(e) = s.config.save()
            {
                s.error = e;
            }
            s.dirty = true;
            let page = s.preferences.as_ref().map(|p| p.page);
            drop(s);
            if let Some(p) = page {
                show_preferences(state, p);
            }
        }
        Action::Accessibility => {
            platform::request_accessibility();
            platform::open_url(
                "x-apple.systempreferences:com.apple.preference.security?Privacy_Accessibility",
            );
        }
        Action::ScreenPermission => {
            platform::request_screen();
            platform::open_url(
                "x-apple.systempreferences:com.apple.preference.security?Privacy_ScreenCapture",
            );
        }
        Action::Reset => {
            let mut s = state.borrow_mut();
            let login = s.config.start_at_login;
            let policy = s.config.update_policy.clone();
            let crash = s.config.local_crash_reports;
            s.config = Config::default();
            s.config.start_at_login = login;
            s.config.update_policy = policy;
            s.config.local_crash_reports = crash;
            let _ = s.tx.try_send(Command::Dock(s.config.fully_hide_dock));
            if let Err(e) = s.config.save() {
                s.error = e;
            }
            s.dirty = true;
            drop(s);
            show_preferences(state, 0);
        }
        Action::Diagnostics => write_diagnostics(state),
        Action::Help => {
            if let Some(path) = objc2_foundation::NSBundle::mainBundle().pathForResource_ofType(
                Some(&NSString::from_str("README")),
                Some(&NSString::from_str("md")),
            ) && let Some(url) = objc2_foundation::NSURL::fileURLWithPath(&path).absoluteString()
            {
                platform::open_url(&url.to_string());
            }
        }
        Action::Quit => {
            NSApplication::sharedApplication(MainThreadMarker::new().unwrap()).terminate(None);
        }
    }
}
fn menu_item(s: &Shared, m: &NSMenu, t: &str, a: Action) {
    let item = unsafe {
        NSMenuItem::initWithTitle_action_keyEquivalent(
            NSMenuItem::alloc(MainThreadMarker::new().unwrap()),
            &NSString::from_str(t),
            Some(sel!(menuAction:)),
            &NSString::from_str(""),
        )
    };
    unsafe {
        item.setTarget(Some(s.borrow().delegate()));
    }
    unsafe {
        item.setRepresentedObject(Some(&NSString::from_str(
            &serde_json::to_string(&a).unwrap(),
        )));
    }
    m.addItem(&item);
}
fn context_menu(s: &Shared, a: Option<&Action>) -> Retained<NSMenu> {
    let m = NSMenu::new(MainThreadMarker::new().unwrap());
    if let Some(Action::Window(id) | Action::Bubble(id)) = a {
        let w = s
            .borrow()
            .snapshot
            .windows
            .iter()
            .find(|w| w.id == *id)
            .cloned();
        if let Some(w) = w {
            for (t, a) in [
                ("Activate Window", Action::Raise(w.id)),
                ("Preview Window", Action::Preview(w.id)),
                ("Minimize / Restore", Action::Minimize(w.id)),
                ("Toggle Full Screen", Action::Fullscreen(w.id)),
                ("Close Window", Action::Close(w.id)),
                ("Keep in Taskbar", Action::Keep(w.bundle.clone())),
                ("Hide Application", Action::HideApp(w.pid)),
                ("Quit Application", Action::QuitApp(w.pid)),
                ("Blacklist Application", Action::Blacklist(w.bundle)),
            ] {
                menu_item(s, &m, t, a);
            }
        }
    } else if let Some(Action::Tab(id, index)) = a {
        menu_item(s, &m, "Select Tab", Action::Tab(*id, *index));
        menu_item(s, &m, "Close Tab", Action::CloseTab(*id, *index));
    } else if let Some(Action::Pin(b, a)) = a {
        menu_item(s, &m, "Open Application", Action::Pin(b.clone(), a.clone()));
        menu_item(
            s,
            &m,
            "New Window",
            Action::Pin(b.clone(), "newWindow".into()),
        );
        let private = match b.as_str() {
            "com.google.Chrome" => Some(("New Incognito Window", "googleChromeNewIncognitoWindow")),
            "com.apple.Safari" => Some(("New Private Window", "safariNewPrivateWindow")),
            "org.mozilla.firefox" => Some(("New Private Window", "firefoxNewPrivateWindow")),
            _ => None,
        };
        if let Some((title, action)) = private {
            menu_item(s, &m, title, Action::Pin(b.clone(), action.into()));
            menu_item(
                s,
                &m,
                &format!("Click: {title}"),
                Action::PinBehavior(b.clone(), action.into()),
            );
        }
        let pinned = s.borrow().config.pins.iter().any(|p| p.bundle == *b);
        menu_item(
            s,
            &m,
            if pinned {
                "Remove from Taskbar"
            } else {
                "Keep in Taskbar"
            },
            if pinned {
                Action::Unpin(b.clone())
            } else {
                Action::Keep(b.clone())
            },
        );
        for (t, a) in [
            ("Click: launch or activate", "launchOrActivateApp"),
            ("Click: new window", "newWindow"),
        ] {
            menu_item(s, &m, t, Action::PinBehavior(b.clone(), a.into()));
        }
    } else if let Some(Action::Start(_)) = a {
        menu_item(
            s,
            &m,
            "Toggle Start / Spotlight Apps",
            Action::Cycle("start_action".into()),
        );
    }
    if matches!(a, Some(Action::Sort)) {
        menu_item(s, &m, "Sort Windows", Action::Sort);
        menu_item(s, &m, "Edit Application Order…", Action::Preferences(10));
    }
    m.addItem(&NSMenuItem::separatorItem(MainThreadMarker::new().unwrap()));
    if let Some(Action::HideBar(d, _)) = a {
        menu_item(s, &m, "Hide This Taskbar", Action::HideBar(*d, false));
        menu_item(s, &m, "Hide for Now", Action::HideBar(*d, true));
    }
    menu_item(s, &m, "Show All Taskbars", Action::ShowBars);
    menu_item(s, &m, "Hide All Taskbars", Action::HideBars);
    if !matches!(a, Some(Action::Sort)) {
        menu_item(s, &m, "Sort Windows", Action::Sort);
        menu_item(s, &m, "Edit Application Order…", Action::Preferences(10));
    }
    menu_item(s, &m, "Preferences…", Action::Preferences(0));
    menu_item(s, &m, "About Rowla", Action::Preferences(9));
    menu_item(s, &m, "Rowla Help", Action::Help);
    menu_item(s, &m, "Save Diagnostics…", Action::Diagnostics);
    menu_item(s, &m, "Quit Rowla", Action::Quit);
    m
}
fn reorder(state: &Shared, a: &Action, x: f64, window: isize) -> bool {
    let mut s = state.borrow_mut();
    let to = s
        .bars
        .iter()
        .find(|b| b.panel.windowNumber() == window)
        .and_then(|b| {
            b.buttons.values().find(|v| {
                let r = v.convertRect_toView(v.bounds(), None);
                matches!(v.ivars().action, Action::Window(_) | Action::Pin(..))
                    && x >= r.origin.x
                    && x < r.origin.x + r.size.width
            })
        })
        .map(|b| b.ivars().action.clone());
    match (a, to) {
        (Action::Window(from), Some(Action::Window(to)))
            if s.config.task_dragging && *from != to =>
        {
            if let (Some(a), Some(b)) = (
                s.order.iter().position(|id| id == from),
                s.order.iter().position(|id| *id == to),
            ) {
                let mut moving = vec![*from];
                if let Some(button) = s
                    .bars
                    .iter()
                    .find_map(|bar| bar.buttons.get(&Action::Window(*from)))
                {
                    moving.extend(button.ivars().related_ids.borrow().iter().copied());
                }
                let target = s.order[b];
                let after = a < b;
                s.order.retain(|id| !moving.contains(id));
                let at = s
                    .order
                    .iter()
                    .position(|id| *id == target)
                    .unwrap_or(s.order.len());
                let at = (at + usize::from(after)).min(s.order.len());
                s.order.splice(at..at, moving);
                s.dirty = true;
                return true;
            }
        }
        (Action::Pin(from, _), Some(Action::Pin(to, _)))
            if s.config.pin_dragging && *from != to =>
        {
            if let (Some(a), Some(b)) = (
                s.config.pins.iter().position(|p| &p.bundle == from),
                s.config.pins.iter().position(|p| p.bundle == to),
            ) {
                let p = s.config.pins.remove(a);
                s.config.pins.insert(b, p);
                if let Err(e) = s.config.save() {
                    s.error = e;
                }
                s.dirty = true;
                return true;
            }
        }
        _ => (),
    }
    false
}
fn begin_hover(state: &Shared, id: u32, window: isize, anchor: NSRect) {
    let timer_needed = {
        let Ok(mut s) = state.try_borrow_mut() else {
            return;
        };
        if !s.config.thumbnails || !s.snapshot.screen_allowed {
            return;
        }
        if s.hover.is_some_and(|h| h.0 == id) {
            s.hover_anchor = anchor;
            return;
        }
        poll_images(&mut s);
        if s.capture_busy && s.capture_source != Some(id) {
            crate::capture::cancel_pending();
        }
        let display = s
            .bars
            .iter()
            .find(|b| b.panel.windowNumber() == window)
            .map(|b| b.display)
            .unwrap_or(0);
        // A ready replacement can paint in the same visible panel. Hiding it
        // first creates a visible break when moving between neighboring tiles.
        if s.preview_id != Some(id)
            && !(s.config.hover_ms == 0 && s.preview_cache.get(id).is_some())
        {
            if let Some(p) = &s.preview {
                p.panel.orderOut(None);
                p.image.setImage(None);
            }
            s.preview_id = None;
        }
        s.hover = Some((id, display, Instant::now()));
        s.hover_anchor = anchor;
        s.interaction_timer.is_none()
    };
    if timer_needed {
        let delegate = state.borrow().delegate;
        if let Some(delegate) = delegate {
            let timer = unsafe {
                // SAFETY: the retained delegate owner outlives this timer,
                // which is invalidated in applicationWillTerminate.
                NSTimer::scheduledTimerWithTimeInterval_target_selector_userInfo_repeats(
                    0.02,
                    delegate.as_ref(),
                    sel!(interactionTick:),
                    None,
                    true,
                )
            };
            state.borrow_mut().interaction_timer = Some(timer);
        }
    }
    // Ready thumbnails are displayed in mouseEntered itself. A cold capture
    // starts here too, rather than waiting for the next timer or hover delay.
    update_hover(state);
}
fn prune_preview_sources(s: &mut State, alive: &HashSet<u32>) {
    if s.config.thumbnails && s.snapshot.screen_allowed {
        s.preview_cache.retain(|id| alive.contains(&id));
    } else {
        s.preview_cache.clear();
    }
    s.prefetch_retry.retain(|(id, _)| alive.contains(id));
}
fn poll_images(s: &mut State) {
    while let Ok(result) = s.image_rx.try_recv() {
        s.capture_busy = false;
        s.capture_source = None;
        s.performance.capture.record(s.capture_started.elapsed());
        match result {
            Ok((id, image)) => {
                s.captures += 1;
                if s.config.thumbnails
                    && s.snapshot.screen_allowed
                    && s.snapshot.windows.iter().any(|w| w.id == id)
                {
                    // SAFETY: Image owns a retained immutable CGImage from the
                    // capture engine; it stays alive throughout these queries.
                    let bitmap = unsafe { &*image.0.cast::<objc2_core_graphics::CGImage>() };
                    let bytes = objc2_core_graphics::CGImage::bytes_per_row(Some(bitmap))
                        .saturating_mul(objc2_core_graphics::CGImage::height(Some(bitmap)));
                    s.preview_cache
                        .insert(id, image.clone(), bytes, Instant::now());
                    if s.hover.is_some_and(|h| {
                        h.0 == id && h.2.elapsed() >= Duration::from_millis(s.config.hover_ms)
                    }) {
                        show_preview_image(s, id, image);
                    }
                }
            }
            Err(e) if e == crate::capture::CANCELLED => {
                s.performance.capture_cancelled += 1;
            }
            Err(e) => {
                s.capture_errors += 1;
                s.error = e;
            }
        }
    }
}
fn interaction_tick(state: &Shared) {
    // Drain native temporaries even when AppKit waits inside its event loop
    // while the 50 Hz interaction timer fires without any physical input.
    autoreleasepool(|_| {
        poll_images(&mut state.borrow_mut());
        update_hover(state);
        let mut s = state.borrow_mut();
        if s.hover.is_none()
            && !s.capture_busy
            && s.preview_id.is_none()
            && let Some(timer) = s.interaction_timer.take()
        {
            timer.invalidate();
        }
    });
}
fn hide_preview(s: &Shared) {
    let mut s = s.borrow_mut();
    s.hover = None;
    s.preview_id = None;
    s.preview_until = None;
    if let Some(p) = &s.preview {
        p.panel.orderOut(None);
        p.image.setImage(None);
    }
}
fn update_hover(state: &Shared) {
    let mut s = state.borrow_mut();
    // Permission/settings cleanup must run even when mouseExited cleared hover
    // while the pointer remains in the popup. Late frames cannot revive it.
    if !s.config.thumbnails || !s.snapshot.screen_allowed {
        if s.capture_busy {
            crate::capture::cancel_pending();
        }
        s.hover = None;
        s.preview_until = None;
        s.preview_cache.clear();
        s.prefetch_retry.clear();
        s.preview_id = None;
        if let Some(p) = &s.preview {
            p.panel.orderOut(None);
            p.image.setImage(None);
        }
        return;
    }
    if s.capture_busy && s.capture_started.elapsed() > Duration::from_secs(15) {
        s.error =
            "Preview capture timed out. Restart Rowla to retry. Window controls still work.".into();
    }
    if s.preview_until.is_some_and(|t| Instant::now() > t) {
        s.hover = None;
        s.preview_until = None;
    }
    let Some((id, _, since)) = s.hover else {
        if s.preview_id.is_none() {
            return;
        }
        if let Some(p) = &s.preview {
            let cursor = NSEvent::mouseLocation();
            let f = p.panel.frame();
            if !(cursor.x >= f.origin.x
                && cursor.x <= f.origin.x + f.size.width
                && cursor.y >= f.origin.y - 10.0
                && cursor.y <= f.origin.y + f.size.height)
            {
                p.panel.orderOut(None);
                p.image.setImage(None);
                s.preview_id = None;
            }
        }
        return;
    };
    let display_ready = since.elapsed() >= Duration::from_millis(s.config.hover_ms);
    if display_ready
        && s.preview_id != Some(id)
        && let Some(image) = s.preview_cache.get(id)
    {
        let image = image.clone();
        show_preview_image(&mut s, id, image);
        s.performance.cached_hover.record(since.elapsed());
    }
    // macOS pauses single-window streams while minimized. The last copied
    // bitmap remains useful, but requesting a new one can only wait or fail.
    if s.snapshot.windows.iter().any(|w| w.id == id && w.minimized) {
        if s.capture_busy && s.capture_source == Some(id) {
            crate::capture::cancel_pending();
        }
        return;
    }
    if s.capture_busy {
        return;
    }
    if !s.capture_gate.allow(id, Instant::now()) {
        return;
    }
    drop(s);
    request_preview(state, id);
}
fn preview_size(s: &State, id: u32) -> (isize, isize) {
    preview_dimensions(
        s.config.thumbnail_scale,
        s.snapshot.windows.iter().find(|w| w.id == id),
    )
}
fn preview_dimensions(scale: f64, window: Option<&Window>) -> (isize, isize) {
    let width = (560.0 * scale / 100.0) as isize;
    let height = window
        .map(|w| (width as f64 * w.height / w.width.max(1.0)) as isize)
        .unwrap_or(400)
        .clamp(40, 800);
    (width, height)
}
fn request_preview(state: &Shared, id: u32) {
    let mut s = state.borrow_mut();
    s.capture_busy = true;
    s.capture_source = Some(id);
    s.capture_started = Instant::now();
    let tx = s.image_tx.clone();
    let (width, height) = preview_size(&s, id);
    let mode = s.config.capture_mode;
    drop(s);
    platform::screenshot(id, width, height, tx, mode);
}
fn task_button_visible(button: &ActionButton, scroll: &TaskScrollView) -> bool {
    let clip = scroll.contentView();
    let frame = button.convertRect_toView(button.bounds(), Some(&clip));
    !button.isHidden() && objc2_foundation::NSIntersectsRect(frame, clip.bounds())
}
fn prefetch_preview(state: &Shared) {
    let mut s = state.borrow_mut();
    let now = Instant::now();
    if s.capture_busy
        || s.hover.is_some()
        || s.preview_id.is_some()
        || !s.config.thumbnails
        || !s.snapshot.screen_allowed
        || now < s.prefetch_after
        || (s.gui_smoke.is_some() && s.hover_benchmark.is_none())
    {
        return;
    }
    s.prefetch_retry.retain(|(_, until)| *until > now);
    // Warm only visible source windows, prioritizing the focused source. Hidden native
    // QA is restricted to its disposable fixture, never other desktop windows.
    let candidates = s.snapshot.windows.iter().filter(|w| {
        !w.minimized
            && !w.hidden
            && w.on_space
            && s.hover_benchmark.as_ref().is_none_or(|test| test.owns(w))
            && s.preview_cache.needs_refresh(w.id, now)
            && !s.prefetch_retry.iter().any(|(id, _)| *id == w.id)
            && {
                let (width, height) = preview_dimensions(s.config.thumbnail_scale, Some(w));
                let bytes = (width as usize * 4 + 64) * height as usize;
                s.preview_cache.can_prefetch(w.id, bytes)
            }
            && s.bars.iter().any(|bar| {
                (s.hover_benchmark.is_some() || bar.panel.isVisible())
                    && bar.buttons.get(&Action::Window(w.id)).is_some_and(|b| {
                        s.hover_benchmark.is_some() || task_button_visible(b, &bar.scroll)
                    })
            })
    });
    let mut next = None;
    for window in candidates {
        if next.is_none() || window.focused {
            next = Some(window.id);
        }
        if window.focused {
            break;
        }
    }
    if let Some(id) = next {
        s.prefetch_after = now + Duration::from_millis(500);
        // An unavailable window must not monopolize background warming. Hover
        // requests bypass this per-source retry deadline.
        s.prefetch_retry
            .push_back((id, now + Duration::from_secs(30)));
        while s.prefetch_retry.len() > preview_cache::MAX_ENTRIES {
            s.prefetch_retry.pop_front();
        }
        drop(s);
        request_preview(state, id);
    }
}
fn show_preview_image(s: &mut State, id: u32, image: platform::Image) {
    let first_display = s.preview_id != Some(id);
    let Some(p) = &s.preview else { return };
    let img = unsafe {
        NSImage::initWithCGImage_size(
            NSImage::alloc(),
            &*image.0.cast::<objc2_core_graphics::CGImage>(),
            NSSize::new(0.0, 0.0),
        )
    };
    let size = img.size();
    let w = 280.0 * s.config.thumbnail_scale / 100.0;
    let h = (w * size.height / size.width.max(1.0)).clamp(50.0, 400.0);
    let th = preview_frame::header_height(s.config.thumbnail_font);
    let inset = preview_frame::INSET;
    let frame = rect(
        s.bars
            .iter()
            .find(|b| contains(b.frame, s.hover_anchor.origin))
            .map(|b| {
                s.hover_anchor.origin.x.clamp(
                    b.frame.origin.x,
                    (b.frame.origin.x + b.frame.size.width - w).max(b.frame.origin.x),
                )
            })
            .unwrap_or(s.hover_anchor.origin.x),
        s.hover_anchor.origin.y + s.hover_anchor.size.height,
        w,
        h + th + inset * 2.0,
    );
    if p.panel.frame() != frame {
        p.panel.setFrame_display(frame, false);
    }
    if first_display {
        p.frame.clear_highlight();
    }
    layout_preview(p, w, h, s.config.thumbnail_font);
    p.image.setImage(Some(&img));
    let hide_title = !s.config.thumbnail_titles;
    if p.title.isHidden() != hide_title {
        p.title.setHidden(hide_title);
    }
    let title = s
        .snapshot
        .windows
        .iter()
        .find(|w| w.id == id)
        .map(|w| w.title.as_str())
        .unwrap_or_default();
    if !native_text_matches(&p.title.stringValue(), &[title]) {
        p.title.setStringValue(&NSString::from_str(title));
    }
    if !p.panel.isVisible() {
        p.panel.orderFrontRegardless();
    }
    p.panel.displayIfNeeded();
    s.preview_id = Some(id);
    if first_display && let Some((_, _, since)) = s.hover {
        s.performance.hover.record(since.elapsed());
    }
}
fn layout_preview(p: &Preview, width: f64, image_height: f64, font: f64) {
    let inset = preview_frame::INSET;
    let header = preview_frame::header_height(font);
    if p.title
        .font()
        .is_none_or(|current| current.pointSize() != font)
    {
        p.title.setFont(Some(&NSFont::systemFontOfSize(font)));
    }
    for (view, frame) in [
        (
            &*p.image as &NSView,
            rect(inset, inset, width - inset * 2.0, image_height),
        ),
        (
            &*p.title as &NSView,
            rect(6.0, image_height + inset, width - 32.0, header),
        ),
        (
            &*p.close as &NSView,
            rect(
                width - 22.0,
                image_height + inset + (header - 18.0) / 2.0,
                18.0,
                18.0,
            ),
        ),
    ] {
        if view.frame() != frame {
            view.setFrame(frame);
        }
    }
}
fn initialize_preview(s: &Shared) {
    let p = panel(rect(0.0, 0.0, 280.0, 200.0), "Window Preview", false);
    p.setHasShadow(true);
    p.setOpaque(false);
    p.setBackgroundColor(Some(&NSColor::clearColor()));
    let root = preview_frame::PreviewFrame::new(rect(0.0, 0.0, 280.0, 200.0));
    p.setContentView(Some(&root));
    let image = NSImageView::initWithFrame(
        NSImageView::alloc(MainThreadMarker::new().unwrap()),
        rect(0.0, 0.0, 280.0, 170.0),
    );
    image.setImageScaling(NSImageScaling::ScaleProportionallyUpOrDown);
    image.setImageFrameStyle(NSImageFrameStyle::None);
    let title = label("", rect(0.0, 170.0, 280.0, 25.0), 13.0);
    title.setUsesSingleLineMode(true);
    title.setLineBreakMode(NSLineBreakMode::ByTruncatingTail);
    let close = ActionButton::new(s, Action::ClosePreview, "×", rect(256.0, 174.0, 18.0, 18.0));
    close.setBordered(false);
    close.setFont(Some(&NSFont::systemFontOfSize(12.0)));
    close.setToolTip(Some(&NSString::from_str("Close previewed window")));
    {
        root.addSubview(&image);
        root.addSubview(&title);
        root.addSubview(&close);
    }
    s.borrow_mut().preview = Some(Preview {
        panel: p,
        frame: root,
        image,
        title,
        close,
    });
}

// Preference and Start-menu builders are kept below the event/lifetime code.
fn bool_setting<'a>(c: &'a mut Config, k: &str) -> Option<&'a mut bool> {
    Some(match k {
        "start_at_login" => &mut c.start_at_login,
        "show_start" => &mut c.show_start,
        "show_sort" => &mut c.show_sort,
        "scroll_down_hides" => &mut c.scroll_down_hides,
        "scroll_up_hides" => &mut c.scroll_up_hides,
        "middle_closes" => &mut c.middle_closes,
        "show_hidden" => &mut c.show_hidden,
        "show_badges" => &mut c.show_badges,
        "chrome_profile_badges" => &mut c.chrome_profile_badges,
        "show_all_spaces" => &mut c.show_all_spaces,
        "show_tabs" => &mut c.show_tabs,
        "compact_related_windows" => &mut c.compact_related_windows,
        "group_by_app" => &mut c.group_by_app,
        "task_dragging" => &mut c.task_dragging,
        "pin_dragging" => &mut c.pin_dragging,
        "click_hides_app" => &mut c.click_hides_app,
        "all_displays" => &mut c.all_displays,
        "main_only" => &mut c.main_only,
        "center" => &mut c.center,
        "show_titles" => &mut c.show_titles,
        "thumbnail_titles" => &mut c.thumbnail_titles,
        "indicate_minimized" => &mut c.indicate_minimized,
        "indicate_hidden" => &mut c.indicate_hidden,
        "show_menubar" => &mut c.show_menubar,
        "thumbnails" => &mut c.thumbnails,
        "resize_overlap" => &mut c.resize_overlap,
        "fully_hide_dock" => &mut c.fully_hide_dock,
        "discord_hidden" => &mut c.discord_hidden,
        "reset_space_order" => &mut c.reset_space_order,
        "local_crash_reports" => &mut c.local_crash_reports,
        _ => return None,
    })
}
fn number_setting(c: &mut Config, k: &str, v: f64) {
    match k {
        "scale" => c.scale = v,
        "font_size" => c.font_size = v,
        "thumbnail_scale" => c.thumbnail_scale = v,
        "thumbnail_font" => c.thumbnail_font = v,
        "start_scale" => c.start_scale = v,
        "start_font" => c.start_font = v,
        "transparency" => c.transparency = v,
        "max_width" => c.max_width = v,
        "hover_ms" => c.hover_ms = v as u64,
        _ => (),
    };
    c.normalize();
}
fn show_preferences(state: &Shared, page: usize) {
    build_preferences(state, page);
}
fn refresh_permission_labels(state: &Shared) {
    let s = state.borrow();
    let Some(preferences) = s
        .preferences
        .as_ref()
        .filter(|p| p.page == 3 || p.page == 6)
    else {
        return;
    };
    // Update existing controls without rebuilding or activating the window.
    for view in preferences.body.subviews() {
        let Some(button) = view.downcast_ref::<ActionButton>() else {
            continue;
        };
        let title = match button.ivars().action {
            Action::Accessibility => format!(
                "Accessibility: {} — open settings",
                if s.snapshot.trusted {
                    "Allowed"
                } else {
                    "Required"
                }
            ),
            Action::ScreenPermission => format!(
                "Screen Recording: {} — request / open settings",
                if s.snapshot.screen_allowed {
                    "Allowed"
                } else {
                    "Required for thumbnails"
                }
            ),
            _ => continue,
        };
        let title = NSString::from_str(&title);
        button.setTitle(&title);
        unsafe {
            let _: () = msg_send![button, setAccessibilityLabel: &*title];
        }
    }
}
fn show_start(state: &Shared, display: u32) {
    build_start(state, display);
}
fn update_start_search(state: &Shared) {
    search_start(state);
}
fn write_diagnostics(state: &Shared) {
    let s = state.borrow();
    persist_metrics(&s);
    let p = Config::directory().join("diagnostics.json");
    drop(s);
    platform::open_url(&format!("file://{}", p.to_string_lossy()));
}
fn persist_metrics(s: &State) {
    if s.gui_smoke.is_some() {
        return;
    }
    let (cf_live, cf_peak) = platform::cf_counts();
    let (frames, sessions, live, running) = crate::capture::counts();
    let mut d = serde_json::json!({"version":env!("CARGO_PKG_VERSION"),"pid":std::process::id(),"accessibility":s.snapshot.trusted,"screen_recording":s.snapshot.screen_allowed,"hover_close_enabled":s.hover_close.as_ref().is_some_and(hover_close::Service::enabled),"login_status":platform::login_status(),"windows":s.snapshot.windows.len(),"icons":s.icons.len(),"order_entries":s.order.len(),"displays":s.bars.len(),"installed_apps":s.apps.len(),"captures":s.captures,"capture_errors":s.capture_errors,"capture_pending":s.capture_busy,"scan_ms":s.snapshot.scan_ms,"capabilities":s.snapshot.capabilities,"discovery":s.snapshot.discovery,"cf_live":cf_live,"cf_peak":cf_peak,"stream_frames":frames,"stream_sessions":sessions,"stream_live":live,"stream_running":running,"capture_stop_errors":crate::capture::stop_errors(),"capture_native_state_uncertain":crate::capture::native_uncertain(),"performance":s.performance.json(),"preview_cache_entries":s.preview_cache.len(),"interaction_timer_active":s.interaction_timer.is_some(),"error":s.error});
    d["capture_features"] = crate::capture::diagnostics();
    d["chrome_profiles"] = s.profile_badges.diagnostics();
    d["capture_mode"] = serde_json::to_value(s.config.capture_mode).unwrap();
    d["minimum_macos"] = "15.2".into();
    let _ = std::fs::write(
        Config::directory().join("diagnostics.json"),
        serde_json::to_vec_pretty(&d).unwrap(),
    );
}
fn release_closed_preferences(s: &mut State) {
    // Release settings controls, icons and backing surfaces after closing.
    // Miniaturizing keeps the window available to restore.
    if s.preferences
        .as_ref()
        .is_some_and(|p| !p.panel.isVisible() && !p.panel.isMiniaturized())
        && let Some(p) = s.preferences.take()
    {
        clear(&p.body);
        p.panel.close();
        s.dirty = true;
    }
}

fn build_preferences(state: &Shared, page: usize) {
    let m = MainThreadMarker::new().unwrap();
    let page = page.min(10);
    if state.borrow().preferences.is_none() {
        let p = unsafe {
            NSWindow::initWithContentRect_styleMask_backing_defer(
                NSWindow::alloc(m),
                rect(0.0, 0.0, 820.0, 700.0),
                NSWindowStyleMask::Titled
                    | NSWindowStyleMask::Closable
                    | NSWindowStyleMask::Miniaturizable,
                NSBackingStoreType::Buffered,
                false,
            )
        };
        unsafe {
            p.setReleasedWhenClosed(false);
        }
        p.setTitle(&NSString::from_str("Rowla — Preferences"));
        p.center();
        let root = p.contentView().unwrap();
        crate::native_features::compact_controls(&root);
        for (i, t) in [
            "Taskbar",
            "Appearance",
            "Menubar",
            "Thumbnails",
            "Blacklist",
            "Advanced",
            "Permissions",
            "Policies",
            "Reset",
            "About",
            "Sort",
        ]
        .iter()
        .enumerate()
        {
            let b = ActionButton::new(
                state,
                Action::Preferences(i),
                t,
                rect(8.0 + i as f64 * 73.0, 646.0, 71.0, 28.0),
            );
            b.setFont(Some(&NSFont::systemFontOfSize(12.0)));
            {
                root.addSubview(&b);
            }
        }
        let scroll =
            NSScrollView::initWithFrame(NSScrollView::alloc(m), rect(16.0, 16.0, 788.0, 615.0));
        scroll.setHasVerticalScroller(true);
        scroll.setDrawsBackground(false);
        let body = NSView::initWithFrame(NSView::alloc(m), rect(0.0, 0.0, 766.0, 615.0));
        scroll.setDocumentView(Some(&body));
        {
            root.addSubview(&scroll);
        }
        state.borrow_mut().preferences = Some(Preferences {
            panel: p,
            body,
            page,
            targets: vec![],
        });
    }
    let (body, panel, mut c) = {
        let mut s = state.borrow_mut();
        let c = s.config.clone();
        let p = s.preferences.as_mut().unwrap();
        p.page = page;
        p.targets.clear();
        (p.body.clone(), p.panel.clone(), c)
    };
    clear(&body);
    set_theme(&panel, &c.theme);
    let controls = match page {
        0 => vec![
            ("start_at_login", "Start at Login"),
            ("show_start", "Show Start button"),
            ("show_sort", "Show Sort button"),
            ("scroll_down_hides", "Scrolling down hides for now"),
            ("scroll_up_hides", "Scrolling up hides for now"),
            ("middle_closes", "Middle-click closes window"),
            ("show_hidden", "Show hidden windows"),
            ("show_badges", "Show app notification badges"),
            ("chrome_profile_badges", "Show Chrome profile badges"),
            ("show_all_spaces", "Indicate apps assigned to all Spaces"),
            (
                "compact_related_windows",
                "Show related windows as compact bubbles",
            ),
            (
                "show_tabs",
                "Show inactive native tabs as separate tiles when ungrouped",
            ),
            ("group_by_app", "Group tasks by app chronologically"),
            ("task_dragging", "Reorder tasks by dragging"),
            ("pin_dragging", "Reorder pinned apps by dragging"),
            ("click_hides_app", "Focused task click hides application"),
            (
                "all_displays",
                "Show all displays' windows on every taskbar",
            ),
            ("main_only", "Show taskbar on main display only"),
        ],
        1 => vec![
            ("center", "Center tasks and buttons"),
            ("show_titles", "Show window titles in tasks"),
            ("thumbnail_titles", "Show thumbnail window titles"),
            (
                "indicate_minimized",
                "Indicate minimized windows with [brackets]",
            ),
            (
                "indicate_hidden",
                "Indicate hidden windows with (parentheses)",
            ),
        ],
        2 => vec![("show_menubar", "Show menubar icon")],
        3 => vec![
            ("thumbnails", "Show window thumbnail previews"),
            ("thumbnail_titles", "Show thumbnail window titles"),
        ],
        5 => vec![
            ("resize_overlap", "Resize windows overlapping Taskbar"),
            ("fully_hide_dock", "Fully hide Dock while running"),
            ("discord_hidden", "Do not show hidden Discord windows"),
            (
                "reset_space_order",
                "Reset order on Space/fullscreen changes",
            ),
        ],
        7 => vec![("local_crash_reports", "Keep crash reports locally")],
        _ => vec![],
    };
    let heading = [
        "Taskbar",
        "Appearance",
        "Menubar Icon",
        "Thumbnails",
        "Blacklist",
        "Advanced",
        "Permissions",
        "Policies",
        "Reset",
        "About",
        "Application sort order",
    ][page];
    let content_height = if page == 0 {
        655.0
    } else if page == 10 {
        (c.app_order.len() as f64 * 36.0 + 210.0).max(615.0)
    } else {
        615.0
    };
    body.setFrameSize(NSSize::new(body.frame().size.width, content_height));
    let mut y = content_height - 35.0;
    let t = label(heading, rect(8.0, y, 730.0, 28.0), 22.0);
    {
        body.addSubview(&t);
    }
    y -= 42.0;
    if page == 10 {
        sort_order::build_editor(state, &body, &c, y);
        body.scrollPoint(NSPoint::new(0.0, content_height));
    }
    for (key, title) in controls {
        let value = bool_setting(&mut c, key).is_some_and(|v| *v);
        let b = ActionButton::new(
            state,
            Action::Toggle(key.into()),
            title,
            rect(8.0, y, 730.0, 28.0),
        );
        b.setButtonType(NSButtonType::Switch);
        b.setState(if value { 1 } else { 0 });
        {
            body.addSubview(&b);
        }
        y -= 34.0;
    }
    if page == 2 {
        let b = ActionButton::new(
            state,
            Action::ShowBars,
            "Show all taskbars",
            rect(8.0, y, 240.0, 32.0),
        );
        body.addSubview(&b);
        y -= 42.0;
        let hide = ActionButton::new(
            state,
            Action::HideBars,
            "Hide all taskbars",
            rect(8.0, y, 240.0, 32.0),
        );
        body.addSubview(&hide);
        y -= 60.0;
        let title = label("Shortcuts", rect(8.0, y, 730.0, 28.0), 18.0);
        body.addSubview(&title);
        y -= 65.0;
        let text = label(
            "Install the shortcuts below, then select Add Shortcut in Shortcuts.\nAssign a keyboard shortcut or run them from Spotlight. Each opens one local Rowla URL.",
            rect(8.0, y, 730.0, 52.0),
            13.0,
        );
        body.addSubview(&text);
        y -= 44.0;
        for (index, action, title) in [
            (
                0,
                crate::system_actions::SystemAction::Sort,
                "Install Sort Shortcut…",
            ),
            (
                1,
                crate::system_actions::SystemAction::Show,
                "Install Show Shortcut…",
            ),
            (
                2,
                crate::system_actions::SystemAction::Hide,
                "Install Hide Shortcut…",
            ),
        ] {
            let button = ActionButton::new(
                state,
                Action::InstallShortcut(action),
                title,
                rect(8.0 + index as f64 * 244.0, y, 236.0, 32.0),
            );
            body.addSubview(&button);
        }
        y -= 44.0;
        let shortcuts = ActionButton::new(
            state,
            Action::OpenShortcuts,
            "Open Shortcuts",
            rect(8.0, y, 240.0, 32.0),
        );
        body.addSubview(&shortcuts);
    }
    let values = match page {
        1 => vec![
            ("scale", "Taskbar size (%)", c.scale, 60.0, 200.0),
            ("font_size", "Task font (pt)", c.font_size, 9.0, 24.0),
            (
                "thumbnail_scale",
                "Thumbnail size (%)",
                c.thumbnail_scale,
                50.0,
                200.0,
            ),
            (
                "thumbnail_font",
                "Thumbnail font (pt)",
                c.thumbnail_font,
                9.0,
                24.0,
            ),
            (
                "start_scale",
                "Start menu size (%)",
                c.start_scale,
                60.0,
                160.0,
            ),
            ("start_font", "Start font (pt)", c.start_font, 9.0, 24.0),
            (
                "transparency",
                "Transparency (%)",
                c.transparency,
                0.0,
                95.0,
            ),
            ("max_width", "Max task width (pt)", c.max_width, 60.0, 400.0),
            (
                "hover_ms",
                "Hover delay (ms)",
                c.hover_ms as f64,
                0.0,
                3000.0,
            ),
        ],
        3 => vec![(
            "hover_ms",
            "Hover delay (ms)",
            c.hover_ms as f64,
            0.0,
            3000.0,
        )],
        _ => vec![],
    };
    for (key, title, value, min, max) in values {
        let t = label(
            &format!("{title}: {value:.0}"),
            rect(8.0, y, 300.0, 24.0),
            13.0,
        );
        {
            body.addSubview(&t);
        }
        let slider = NSSlider::initWithFrame(NSSlider::alloc(m), rect(320.0, y, 380.0, 24.0));
        slider.setMinValue(min);
        slider.setMaxValue(max);
        slider.setDoubleValue(value);
        slider.setContinuous(false);
        let target = SettingTarget::new(state, key);
        unsafe {
            slider.setTarget(Some(&target));
            slider.setAction(Some(sel!(changed:)));
            body.addSubview(&slider);
        }
        state
            .borrow_mut()
            .preferences
            .as_mut()
            .unwrap()
            .targets
            .push(target.into_super());
        y -= 32.0;
    }
    if page == 3 {
        let mode = match c.capture_mode {
            crate::config::CaptureMode::Stream => "Stream",
            crate::config::CaptureMode::Snapshot => "Snapshot",
        };
        let button = ActionButton::new(
            state,
            Action::Cycle("capture_mode".into()),
            &format!("Preview capture: {mode}"),
            rect(8.0, y, 300.0, 30.0),
        );
        body.addSubview(&button);
        y -= 44.0;
        let text = label(
            "Stream reuses live frames. Snapshot captures one image per refresh.\nBoth methods keep the same cache limit.",
            rect(8.0, y - 26.0, 730.0, 50.0),
            13.0,
        );
        body.addSubview(&text);
        y -= 68.0;
    }
    if page == 1 || page == 0 || page == 7 {
        let (k, v) = match page {
            1 => ("theme", c.theme.clone()),
            0 => ("start_action", c.start_action.clone()),
            _ => ("update_policy", c.update_policy.clone()),
        };
        let b = ActionButton::new(
            state,
            Action::Cycle(k.into()),
            &format!("{k}: {v} (click to change)"),
            rect(8.0, y, 450.0, 28.0),
        );
        {
            body.addSubview(&b);
        }
        y -= 40.0;
    }
    if page == 4 {
        let add = ActionButton::new(
            state,
            Action::AddBlacklist,
            "Add Application…",
            rect(8.0, y - 34.0, 230.0, 30.0),
        );
        body.addSubview(&add);
        y -= 40.0;
        let t = label(
            "Right-click any task to blacklist an app. Click an entry to remove it.",
            rect(8.0, y, 730.0, 30.0),
            13.0,
        );
        {
            body.addSubview(&t);
        }
        y -= 40.0;
        for b in &c.blacklist {
            let button = ActionButton::new(
                state,
                Action::RemoveBlacklist(b.clone()),
                &format!("Remove: {b}"),
                rect(8.0, y, 700.0, 28.0),
            );
            {
                body.addSubview(&button);
            }
            y -= 34.0;
        }
    }
    if page == 6 || page == 3 {
        for (t, a) in [
            (
                format!(
                    "Accessibility: {} — open settings",
                    if platform::trusted() {
                        "Allowed"
                    } else {
                        "Required"
                    }
                ),
                Action::Accessibility,
            ),
            (
                format!(
                    "Screen Recording: {} — request / open settings",
                    if platform::screen_allowed() {
                        "Allowed"
                    } else {
                        "Required for thumbnails"
                    }
                ),
                Action::ScreenPermission,
            ),
        ] {
            let b = ActionButton::new(state, a, &t, rect(8.0, y, 700.0, 32.0));
            {
                body.addSubview(&b);
            }
            y -= 42.0;
        }
        let t = label(
            "Accessibility lists and controls windows. Screen Recording supplies thumbnails.\nImages stay in memory, with one preview and one capture in flight.\nWindow titles and screenshots are never sent over the network.",
            rect(8.0, y - 72.0, 730.0, 84.0),
            13.0,
        );
        {
            body.addSubview(&t);
        }
        y -= 90.0;
    }
    if page == 7 {
        let t = label(
            "Sparkle verifies signed updates and manages installation.\nChoose manual, automatic checks, or automatic downloads. Crash reports stay on this Mac.",
            rect(8.0, y - 30.0, 730.0, 60.0),
            13.0,
        );
        {
            body.addSubview(&t);
        }
        let check = ActionButton::new(
            state,
            Action::CheckUpdates,
            "Check for Updates",
            rect(8.0, y - 78.0, 230.0, 30.0),
        );
        body.addSubview(&check);
    }
    if page == 8 {
        let b = ActionButton::new(
            state,
            Action::Reset,
            "Reset preferences (retains login and policies)",
            rect(8.0, y, 520.0, 32.0),
        );
        {
            body.addSubview(&b);
        }
    }
    if page == 9 {
        let t = label(
            &format!(
                "Rowla {}\nNative Rust / objc2 / AppKit, for macOS 15.2+.\nIndependent implementation. Original Taskbar remains available.\nInstallation and permissions: Help → README",
                env!("CARGO_PKG_VERSION")
            ),
            rect(8.0, y - 90.0, 730.0, 120.0),
            13.0,
        );
        {
            body.addSubview(&t);
        }
        y -= 140.0;
        let b = ActionButton::new(
            state,
            Action::Diagnostics,
            "Save diagnostics",
            rect(8.0, y, 240.0, 32.0),
        );
        {
            body.addSubview(&b);
        }
    }
    let error = state.borrow().error.clone();
    if !error.is_empty() {
        let t = label(&error, rect(8.0, 5.0, 730.0, 50.0), 11.0);
        {
            body.addSubview(&t);
        }
    }
    let _ = y;
    body.scrollPoint(NSPoint::new(0.0, content_height));
    if state.borrow().gui_smoke.is_none() {
        panel.makeKeyAndOrderFront(None);
        NSApplication::sharedApplication(m).activate();
    }
}
fn build_start(state: &Shared, d: u32) {
    refresh_catalog(state);
    {
        let mut s = state.borrow_mut();
        if let Some(menu) = s.start.take() {
            menu.panel.close();
            return;
        }
    }
    let (f, c) = {
        let s = state.borrow();
        (
            s.bars
                .iter()
                .find(|b| b.display == d)
                .map(|b| b.panel.frame())
                .unwrap_or(rect(0.0, 0.0, 600.0, 32.0)),
            s.config.clone(),
        )
    };
    let scale = c.start_scale / 100.0;
    let w = 560.0 * scale;
    let h = 540.0 * scale;
    let m = MainThreadMarker::new().unwrap();
    let popup: Retained<PopupPanel> = unsafe {
        msg_send![PopupPanel::alloc(m),initWithContentRect:rect(f.origin.x+4.0,f.origin.y+f.size.height+6.0,w,h),styleMask:NSWindowStyleMask::Titled|NSWindowStyleMask::Closable|NSWindowStyleMask::UtilityWindow,backing:NSBackingStoreType::Buffered,defer:false]
    };
    let p = popup.into_super();
    unsafe {
        p.setReleasedWhenClosed(false);
        p.setHidesOnDeactivate(false);
        p.setFloatingPanel(true);
    }
    p.setTitle(&NSString::from_str("Start — Rowla"));
    p.setLevel(3);
    p.setCollectionBehavior(
        NSWindowCollectionBehavior::CanJoinAllSpaces
            | NSWindowCollectionBehavior::FullScreenAuxiliary,
    );
    set_theme(&p, &c.theme);
    let root = p.contentView().unwrap();
    let m = MainThreadMarker::new().unwrap();
    let search = NSSearchField::initWithFrame(
        NSSearchField::alloc(m),
        rect(12.0, h - 42.0, w - 24.0, 28.0),
    );
    search.setPlaceholderString(Some(&NSString::from_str("Search applications")));
    search.setSendsWholeSearchString(true);
    unsafe {
        search.setTarget(Some(state.borrow().delegate()));
        search.setAction(Some(sel!(startSearch:)));
        search.setDelegate(Some(ProtocolObject::from_ref(state.borrow().delegate())));
    }
    {
        root.addSubview(&search);
    }
    let scroll =
        NSScrollView::initWithFrame(NSScrollView::alloc(m), rect(8.0, 8.0, w - 16.0, h - 58.0));
    scroll.setHasVerticalScroller(true);
    scroll.setDrawsBackground(false);
    let body = NSView::initWithFrame(NSView::alloc(m), rect(0.0, 0.0, w - 32.0, h - 58.0));
    scroll.setDocumentView(Some(&body));
    {
        root.addSubview(&scroll);
    }
    if state.borrow().gui_smoke.is_none() {
        p.makeKeyAndOrderFront(None);
    }
    {
        p.makeFirstResponder(Some(&search));
    }
    state.borrow_mut().start = Some(StartMenu {
        panel: p,
        search,
        body,
        query: "\0".into(),
        purpose: PickerPurpose::Launch,
        choices: vec![],
        selected: 0,
    });
    search_start(state);
}
fn refresh_catalog(state: &Shared) {
    let should = {
        let s = state.borrow();
        !s.catalog_busy && s.last_catalog.elapsed() > Duration::from_secs(60)
    };
    if !should {
        return;
    }
    let (tx, rx) = mpsc::sync_channel(1);
    {
        let mut s = state.borrow_mut();
        s.apps_rx = rx;
        s.catalog_busy = true;
        s.last_catalog = Instant::now();
    }
    std::thread::spawn(move || {
        let apps = autoreleasepool(|_| platform::installed_apps());
        let _ = tx.send(apps);
    });
}
fn search_start(state: &Shared) {
    enum Row {
        Heading(&'static str),
        App(Application),
    }
    let values = {
        let s = state.borrow();
        let Some(m) = &s.start else { return };
        let text = m.search.stringValue();
        if native_text_matches(&text, &[&m.query]) {
            return;
        }
        let q = text.to_string();
        let mut rows = vec![];
        if q.is_empty() && m.purpose == PickerPurpose::Launch {
            rows.push(Row::Heading("Pinned"));
            for pin in &s.config.pins {
                if let Some(a) = s.apps.iter().find(|a| a.bundle == pin.bundle) {
                    rows.push(Row::App(a.clone()));
                }
            }
            rows.push(Row::Heading("Recently Used"));
            for bundle in &s.config.recent {
                if let Some(a) = s.apps.iter().find(|a| a.bundle == *bundle) {
                    rows.push(Row::App(a.clone()));
                }
            }
            rows.push(Row::Heading("All Applications"));
        }
        let query = q.to_lowercase();
        for a in &s.apps {
            if m.purpose == PickerPurpose::AppOrder && s.config.app_order.contains(&a.bundle) {
                continue;
            }
            if query.is_empty() || a.name.to_lowercase().contains(&query) {
                rows.push(Row::App(a.clone()));
            }
        }
        (
            q,
            m.body.clone(),
            rows,
            s.config.start_scale,
            s.config.start_font,
            m.purpose,
        )
    };
    let (q, body, rows, scale, font, purpose) = values;
    clear(&body);
    let rowh = 32.0 * scale / 100.0;
    let ch = (rows.len() as f64 * rowh + 12.0).max(480.0);
    let width = body.frame().size.width;
    body.setFrameSize(NSSize::new(width, ch));
    let row_font = NSFont::systemFontOfSize(font);
    let mut y = ch - rowh;
    let mut choices = Vec::with_capacity(rows.len());
    for row in rows {
        match row {
            Row::App(a) => {
                let b = ActionButton::new(
                    state,
                    match purpose {
                        PickerPurpose::Blacklist => Action::Blacklist(a.bundle.clone()),
                        PickerPurpose::AppOrder => Action::AppendOrderApp(a.bundle.clone()),
                        PickerPurpose::Launch => {
                            Action::Pin(a.bundle.clone(), "launchOrActivateApp".into())
                        }
                    },
                    &a.name,
                    rect(4.0, y, width - 8.0, rowh - 2.0),
                );
                b.setFont(Some(&row_font));
                b.ivars().start_index.set(Some(choices.len()));
                choices.push(b.ivars().action.clone());
                b.setAlignment(NSTextAlignment::Left);
                if let Some(i) = icon(state, &a.bundle, &a.path) {
                    b.setImage(Some(&i));
                    b.setImagePosition(NSCellImagePosition::ImageLeft);
                }
                {
                    body.addSubview(&b);
                }
            }
            Row::Heading(name) => {
                let t = label(name, rect(8.0, y + 4.0, width - 16.0, rowh - 8.0), font);
                t.setFont(Some(&NSFont::boldSystemFontOfSize(font)));
                {
                    body.addSubview(&t);
                }
            }
        }
        y -= rowh;
    }
    body.scrollPoint(NSPoint::new(0.0, ch));
    if let Some(m) = state.borrow_mut().start.as_mut() {
        m.query = q;
        m.choices = choices;
        m.selected = 0;
    }
}
fn activate_start_selection(state: &Shared) {
    search_start(state);
    let choice = state
        .borrow()
        .start
        .as_ref()
        .and_then(|m| m.choices.get(m.selected).cloned());
    if let Some(action) = choice {
        dispatch(state, action);
    }
}
fn move_start_selection(state: &Shared, direction: isize) {
    search_start(state);
    let (body, selected) = {
        let mut s = state.borrow_mut();
        let Some(m) = s.start.as_mut() else { return };
        if m.choices.is_empty() {
            return;
        }
        m.selected = m
            .selected
            .saturating_add_signed(direction)
            .min(m.choices.len() - 1);
        (m.body.clone(), m.selected)
    };
    for view in body.subviews() {
        if let Some(b) = view.downcast_ref::<ActionButton>() {
            NSView::setNeedsDisplay(b, true);
            if b.ivars().start_index.get() == Some(selected) {
                body.scrollRectToVisible(b.frame());
            }
        }
    }
}
pub fn run() -> Result<(), String> {
    run_mode(None, None, None)
}
pub(crate) fn gui_smoke(count: usize) -> Result<(), String> {
    run_mode(Some(count.clamp(10, 1000)), None, None)
}
pub(crate) fn benchmark_ui(count: usize) -> Result<(), String> {
    run_mode(Some(0), Some(count.clamp(10, 1000)), None)
}
pub(crate) fn benchmark_hover(count: usize, fixture: i32) -> Result<(), String> {
    run_mode(Some(0), None, Some((count.clamp(2, 100), fixture)))
}
fn run_mode(
    smoke: Option<usize>,
    benchmark: Option<usize>,
    hover: Option<(usize, i32)>,
) -> Result<(), String> {
    if benchmark.is_some() {
        eprintln!("Native UI check: initialize AppKit");
    }
    let mtm = MainThreadMarker::new().expect("main thread");
    let app = NSApplication::sharedApplication(mtm);
    app.setActivationPolicy(NSApplicationActivationPolicy::Accessory);
    let mut config = Config::load()?;
    std::fs::create_dir_all(Config::directory()).map_err(|e| e.to_string())?;
    let _instance = if smoke.is_none() {
        Some(crate::runtime::InstanceGuard::acquire(
            &Config::directory().join("instance.lock"),
        )?)
    } else {
        None
    };
    platform::import_pins(&mut config);
    if !config.app_order_initialized {
        config.app_order = config.pins.iter().map(|pin| pin.bundle.clone()).collect();
        if config.app_order.is_empty() {
            config.app_order = platform::dock_app_order();
        }
        config.app_order_initialized = true;
    }
    config.normalize();
    if hover.is_some() {
        config.hover_ms = 0;
        match std::env::var("ROWLA_QA_CAPTURE_MODE").as_deref() {
            Ok("snapshot") => config.capture_mode = crate::config::CaptureMode::Snapshot,
            Ok("stream") => config.capture_mode = crate::config::CaptureMode::Stream,
            _ => {}
        }
    }
    if smoke.is_none() {
        config.save()?;
    }
    let (tx, worker_rx) = mpsc::sync_channel(64);
    let (worker_tx, rx) = mpsc::sync_channel(1);
    std::thread::spawn(move || platform::worker(worker_rx, worker_tx));
    let (image_tx, image_rx) = mpsc::channel();
    let (apps_tx, apps_rx) = mpsc::channel();
    std::thread::spawn(move || {
        let apps = autoreleasepool(|_| platform::installed_apps());
        let _ = apps_tx.send(apps);
    });
    let state = Rc::new(RefCell::new(State {
        ui_benchmark: benchmark,
        hover_benchmark: hover.map(|(count, pid)| qa::HoverBenchmark::new(count, pid)),
        hover_close: None,
        performance: qa::Performance::default(),
        gui_smoke: smoke.map(|total| GuiSmoke {
            remaining: total,
            total,
            last: Instant::now(),
        }),
        config,
        snapshot: Snapshot::default(),
        pending_focus: None,
        pending_tab: None,
        pending_system_sort: false,
        received_snapshot: false,
        bars: vec![],
        icons: HashMap::new(),
        profile_badges: chrome_profiles::Service::default(),
        order: vec![],
        activity: crate::window_order::Activity::default(),
        sort_order_error: None,
        known_spaces: HashMap::new(),
        hidden_now: HashSet::new(),
        preferences: None,
        start: None,
        preview: None,
        hover: None,
        hover_anchor: rect(0.0, 0.0, 0.0, 0.0),
        preview_cache: preview_cache::PreviewCache::new(),
        image_rx,
        image_tx,
        capture_busy: false,
        capture_source: None,
        prefetch_after: Instant::now(),
        prefetch_retry: std::collections::VecDeque::new(),
        capture_started: Instant::now(),
        capture_gate: crate::runtime::PreviewGate::new(),
        preview_id: None,
        captures: 0,
        capture_errors: 0,
        preview_until: None,
        tx,
        rx,
        apps: vec![],
        apps_rx,
        delegate: None,
        status: None,
        timer: None,
        interaction_timer: None,
        dirty: true,
        last_screens: vec![],
        error: String::new(),
        last_metrics: Instant::now(),
        queue_error: RefCell::new(None),
        updater: crate::updater::Service::new(),
        observed_dark: system_dark(),
        catalog_busy: true,
        last_catalog: Instant::now(),
    }));
    let delegate = Delegate::new(mtm, state.clone());
    // Menus are built before applicationDidFinishLaunching. Their target must
    // already be initialized; this retained delegate lives until app.run returns.
    state.borrow_mut().delegate = Some(NonNull::from(&*delegate));
    app.setDelegate(Some(ProtocolObject::from_ref(&*delegate)));
    let main = NSMenu::new(mtm);
    let application = NSMenu::new(mtm);
    menu_item(&state, &application, "About Rowla", Action::Preferences(9));
    menu_item(&state, &application, "Preferences…", Action::Preferences(0));
    menu_item(&state, &application, "Quit Rowla", Action::Quit);
    if let Some(item) = application.itemAtIndex(1) {
        item.setKeyEquivalent(&NSString::from_str(","));
    }
    if let Some(item) = application.itemAtIndex(2) {
        item.setKeyEquivalent(&NSString::from_str("q"));
    }
    let item = NSMenuItem::new(mtm);
    item.setTitle(&NSString::from_str("Rowla"));
    item.setSubmenu(Some(&application));
    main.addItem(&item);
    app.setMainMenu(Some(&main));
    if benchmark.is_some() {
        eprintln!("Native UI check: enter application event loop");
    }
    autoreleasepool(|_| app.run());
    if (benchmark.is_some() || hover.is_some()) && !state.borrow().error.is_empty() {
        return Err(state.borrow().error.clone());
    }
    Ok(())
}
