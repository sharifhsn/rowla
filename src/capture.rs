//! One bounded capture session, one latest frame, and no per-frame screenshot RPC.
use crate::{config::CaptureMode, platform::Image};
use block2::RcBlock;
use dispatch2::DispatchQueue;
use objc2::{
    AnyThread, DefinedClass, define_class, msg_send,
    rc::{Retained, autoreleasepool},
    runtime::ProtocolObject,
};
use objc2_core_foundation::{CFData, CFRetained};
use objc2_core_graphics::{
    CGBitmapInfo, CGColorRenderingIntent, CGColorSpace, CGDataProvider, CGImage, CGImageAlphaInfo,
    CGImageByteOrderInfo,
};
use objc2_core_media::{CMSampleBuffer, CMTime};
use objc2_core_video::*;
use objc2_foundation::{NSError, NSObject, NSObjectProtocol};
use objc2_screen_capture_kit::{
    SCContentFilter, SCScreenshotManager, SCShareableContent, SCStream, SCStreamConfiguration,
    SCStreamDelegate, SCStreamOutput, SCStreamOutputType,
};
use std::{
    sync::{
        Arc, Mutex, OnceLock,
        atomic::{AtomicBool, AtomicU8, AtomicU64, Ordering},
        mpsc::{self, Sender, SyncSender},
    },
    time::{Duration, Instant},
};

type Response = Sender<Result<(u32, Image), String>>;
struct Request {
    id: u32,
    width: usize,
    height: usize,
    tx: Response,
    generation: u64,
    mode: CaptureMode,
}
enum Command {
    Capture(Request),
    Shutdown(Sender<()>),
}
static ENGINE: OnceLock<SyncSender<Command>> = OnceLock::new();
static FRAMES: AtomicU64 = AtomicU64::new(0);
static SESSIONS: AtomicU64 = AtomicU64::new(0);
static LIVE: AtomicU64 = AtomicU64::new(0);
static ACTIVE: AtomicBool = AtomicBool::new(false);
static STOP_ERRORS: AtomicU64 = AtomicU64::new(0);
static SHUTTING_DOWN: AtomicBool = AtomicBool::new(false);
static STOPPED: AtomicBool = AtomicBool::new(false);
static NATIVE_UNCERTAIN: AtomicBool = AtomicBool::new(false);
static GENERATION: AtomicU64 = AtomicU64::new(0);
static INACTIVE_EVENTS: AtomicU64 = AtomicU64::new(0);
static ACTIVE_EVENTS: AtomicU64 = AtomicU64::new(0);
static NATIVE_CAPTURING: AtomicU8 = AtomicU8::new(0);
static SNAPSHOTS: AtomicU64 = AtomicU64::new(0);
static SNAPSHOT_PENDING: AtomicBool = AtomicBool::new(false);
const SOURCE_INACTIVE: &str = "Preview source is inactive";
pub(crate) fn diagnostics() -> serde_json::Value {
    let native: Option<bool> = match NATIVE_CAPTURING.load(Ordering::Relaxed) {
        1 => Some(false),
        2 => Some(true),
        _ => None,
    };
    serde_json::json!({"source_active_events":ACTIVE_EVENTS.load(Ordering::Relaxed),
        "source_inactive_events":INACTIVE_EVENTS.load(Ordering::Relaxed),
        "native_is_capturing":native,"snapshot_requests":SNAPSHOTS.load(Ordering::Relaxed),
        "snapshot_pending":SNAPSHOT_PENDING.load(Ordering::Relaxed)})
}
fn observe_native(stream: &SCStream) {
    let value = match crate::native_features::native_capture_state(stream) {
        Some(false) => 1,
        Some(true) => 2,
        None => 0,
    };
    NATIVE_CAPTURING.store(value, Ordering::Relaxed);
}
pub(crate) const CANCELLED: &str = "Preview cancelled";
pub(crate) fn cancel_pending() {
    GENERATION.fetch_add(1, Ordering::AcqRel);
}
pub(crate) fn native_uncertain() -> bool {
    NATIVE_UNCERTAIN.load(Ordering::Relaxed)
}
pub fn stop_errors() -> u64 {
    STOP_ERRORS.load(Ordering::Relaxed)
}
pub fn counts() -> (u64, u64, u64, bool) {
    (
        FRAMES.load(Ordering::Relaxed),
        SESSIONS.load(Ordering::Relaxed),
        LIVE.load(Ordering::Relaxed),
        ACTIVE.load(Ordering::Relaxed),
    )
}

#[derive(Default)]
struct FrameSlot {
    image: Option<Image>,
    error: Option<String>,
    enabled: bool,
    inactive: bool,
}
impl FrameSlot {
    fn content_active(&mut self, active: bool) {
        self.inactive = !active;
        if !active {
            self.image = None;
        }
    }
    fn preview(&self) -> Result<Option<Image>, String> {
        if let Some(error) = &self.error {
            return Err(error.clone());
        }
        if self.inactive {
            return Err(SOURCE_INACTIVE.into());
        }
        Ok(self.image.clone())
    }
}
struct OutputIvars {
    slot: Arc<Mutex<FrameSlot>>,
}
define_class!(
    #[unsafe(super = NSObject)]
    #[thread_kind = AnyThread]
    #[ivars = OutputIvars]
    #[name = "TaskbarRustStreamOutput"]
    struct Output;
    unsafe impl NSObjectProtocol for Output {}
    unsafe impl SCStreamOutput for Output {
        #[unsafe(method(stream:didOutputSampleBuffer:ofType:))]
        unsafe fn frame(
            &self,
            _stream: &SCStream,
            sample: &CMSampleBuffer,
            kind: SCStreamOutputType,
        ) {
            if kind != SCStreamOutputType::Screen {
                return;
            }
            autoreleasepool(|_| unsafe {
                if !self
                    .ivars()
                    .slot
                    .lock()
                    .unwrap_or_else(|e| e.into_inner())
                    .enabled
                {
                    return;
                }
                let Some(buffer) = sample.image_buffer() else {
                    return;
                };
                if let Some(image) = copy_frame(&buffer) {
                    let mut slot = self.ivars().slot.lock().unwrap_or_else(|e| e.into_inner());
                    if !slot.enabled || slot.inactive {
                        return;
                    }
                    slot.image = Some(image);
                    slot.error = None;
                    FRAMES.fetch_add(1, Ordering::Relaxed);
                }
            });
        }
    }
    unsafe impl SCStreamDelegate for Output {
        #[unsafe(method(streamDidBecomeInactive:))]
        unsafe fn inactive(&self, _stream: &SCStream) {
            let mut slot = self.ivars().slot.lock().unwrap_or_else(|e| e.into_inner());
            slot.content_active(false);
            INACTIVE_EVENTS.fetch_add(1, Ordering::Relaxed);
            // This is not a stop acknowledgement. Keep the one native session
            // until stop completion or didStopWithError confirms termination.
        }
        #[unsafe(method(streamDidBecomeActive:))]
        unsafe fn active(&self, _stream: &SCStream) {
            let mut slot = self.ivars().slot.lock().unwrap_or_else(|e| e.into_inner());
            slot.content_active(true);
            ACTIVE_EVENTS.fetch_add(1, Ordering::Relaxed);
        }
        #[unsafe(method(stream:didStopWithError:))]
        unsafe fn stopped(&self, _stream: &SCStream, error: &NSError) {
            ACTIVE.store(false, Ordering::Relaxed);
            let mut slot = self.ivars().slot.lock().unwrap_or_else(|e| e.into_inner());
            slot.error = Some(error.localizedDescription().to_string());
            slot.image = None;
            slot.enabled = false;
        }
    }
);
impl Output {
    fn new(slot: Arc<Mutex<FrameSlot>>) -> Retained<Self> {
        unsafe { msg_send![super(Self::alloc().set_ivars(OutputIvars { slot })), init] }
    }
}

struct PixelRead<'a>(&'a CVPixelBuffer);
impl Drop for PixelRead<'_> {
    fn drop(&mut self) {
        unsafe {
            CVPixelBufferUnlockBaseAddress(self.0, CVPixelBufferLockFlags::ReadOnly);
        }
    }
}
fn copy_frame(buffer: &CVPixelBuffer) -> Option<Image> {
    // Copy a thumbnail-sized BGRA bitmap. AppKit's NSImage owns no IOSurface or
    // CMSampleBuffer; the stream keeps at most three native buffers.
    if CVPixelBufferGetPixelFormatType(buffer) != u32::from_be_bytes(*b"BGRA") {
        return None;
    }
    let width = CVPixelBufferGetWidth(buffer);
    let height = CVPixelBufferGetHeight(buffer);
    let stride = CVPixelBufferGetBytesPerRow(buffer);
    let length = stride.checked_mul(height)?;
    if width == 0 || height == 0 || stride < width.checked_mul(4)? || length > 16_000_000 {
        return None;
    }
    let data = unsafe {
        if CVPixelBufferLockBaseAddress(buffer, CVPixelBufferLockFlags::ReadOnly) != 0 {
            return None;
        }
        let _lock = PixelRead(buffer);
        let base = CVPixelBufferGetBaseAddress(buffer);
        if base.is_null() || length > CVPixelBufferGetDataSize(buffer) {
            return None;
        }
        CFData::new(None, base.cast(), length as isize)?
    };
    let provider = CGDataProvider::with_cf_data(Some(&data))?;
    let color = CVImageBufferGetColorSpace(buffer).or_else(CGColorSpace::new_device_rgb)?;
    let info = CGBitmapInfo(
        CGImageAlphaInfo::PremultipliedFirst.0 | CGImageByteOrderInfo::Order32Little.0,
    );
    let image = unsafe {
        CGImage::new(
            width,
            height,
            8,
            32,
            stride,
            Some(&color),
            info,
            Some(&provider),
            std::ptr::null(),
            true,
            CGColorRenderingIntent::RenderingIntentDefault,
        )?
    };
    Some(Image(CFRetained::into_raw(image).as_ptr().cast()))
}

// Immutable SCContentFilter crosses from Apple's content callback to the engine thread.
// No mutation or simultaneous access occurs after it crosses this channel.
struct Filter(Retained<SCContentFilter>);
unsafe impl Send for Filter {}
fn filter(id: u32) -> Result<Filter, String> {
    let (tx, rx) = mpsc::sync_channel(1);
    let block = RcBlock::new(
        move |content: *mut SCShareableContent, error: *mut NSError| {
            autoreleasepool(|_| unsafe {
                let result = if let Some(content) = content.as_ref() {
                    content
                        .windows()
                        .iter()
                        .find(|w| w.windowID() == id)
                        .map(|w| {
                            Filter(SCContentFilter::initWithDesktopIndependentWindow(
                                SCContentFilter::alloc(),
                                &w,
                            ))
                        })
                        .ok_or_else(|| "Window unavailable for preview".into())
                } else {
                    Err(error
                        .as_ref()
                        .map(|e| e.localizedDescription().to_string())
                        .unwrap_or_else(|| "No shareable content".into()))
                };
                let _ = tx.try_send(result);
            });
        },
    );
    unsafe {
        SCShareableContent::getShareableContentExcludingDesktopWindows_onScreenWindowsOnly_completionHandler(true, false, &block);
    }
    rx.recv_timeout(Duration::from_secs(5))
        .map_err(|_| "Window listing timed out".to_string())?
}
fn completion(f: impl FnOnce(&RcBlock<dyn Fn(*mut NSError)>)) -> Result<(), String> {
    let (tx, rx) = mpsc::sync_channel(1);
    let block = RcBlock::new(move |error: *mut NSError| {
        let result = unsafe { error.as_ref() }
            .map(|e| {
                Err(format!(
                    "{} ({}: {})",
                    e.localizedDescription(),
                    e.domain(),
                    e.code()
                ))
            })
            .unwrap_or(Ok(()));
        let _ = tx.try_send(result);
    });
    f(&block);
    rx.recv_timeout(Duration::from_secs(5))
        .map_err(|_| "Capture operation timed out".to_string())?
}
fn configuration(r: &Request) -> Retained<SCStreamConfiguration> {
    // SAFETY: each configuration belongs to the serial capture worker. Its
    // dimensions are clamped before enqueueing; SDR and silent capture remain.
    unsafe {
        let config = SCStreamConfiguration::new();
        config.setWidth(r.width);
        config.setHeight(r.height);
        config.setMinimumFrameInterval(CMTime::new(1, 1));
        config.setQueueDepth(3);
        config.setPixelFormat(u32::from_be_bytes(*b"BGRA"));
        config.setShowsCursor(false);
        config.setCapturesAudio(false);
        config.setIgnoreShadowsSingleWindow(true);
        config
    }
}

// A timed-out screenshot keeps its filter/configuration and receiver alive.
// No subsequent screenshot or stream starts until its callback completes.
struct PendingSnapshot {
    rx: mpsc::Receiver<Result<Image, String>>,
    enabled: Arc<AtomicBool>,
    _filter: Filter,
    _config: Retained<SCStreamConfiguration>,
}
impl PendingSnapshot {
    fn start(r: &Request) -> Result<Self, String> {
        let filter = filter(r.id)?;
        let config = configuration(r);
        let (tx, rx) = mpsc::sync_channel(1);
        let enabled = Arc::new(AtomicBool::new(true));
        let accepting = enabled.clone();
        let generation = r.generation;
        let width = r.width;
        let height = r.height;
        let block = RcBlock::new(move |image: *mut CGImage, error: *mut NSError| {
            autoreleasepool(|_| {
                let result = if !accepting.load(Ordering::Acquire)
                    || GENERATION.load(Ordering::Acquire) != generation
                    || SHUTTING_DOWN.load(Ordering::Acquire)
                {
                    Err(CANCELLED.into())
                } else if let Some(error) = unsafe { error.as_ref() } {
                    Err(error.localizedDescription().to_string())
                } else {
                    // SAFETY: Apple lends the image for this completion call.
                    // Copy bounded SDR pixels before the callback returns.
                    unsafe { image.as_ref() }
                        .and_then(|image| copy_screenshot(image, width, height))
                        .ok_or_else(|| "Screenshot has no bounded SDR image".into())
                };
                let _ = tx.try_send(result);
            });
        });
        SNAPSHOTS.fetch_add(1, Ordering::Relaxed);
        SNAPSHOT_PENDING.store(true, Ordering::Relaxed);
        // SAFETY: macOS 15.2 includes this macOS 14 API. PendingSnapshot keeps
        // immutable inputs alive; Apple copies the asynchronous completion.
        unsafe {
            SCScreenshotManager::captureImageWithFilter_configuration_completionHandler(
                &filter.0,
                &config,
                Some(&block),
            );
        }
        Ok(Self {
            rx,
            enabled,
            _filter: filter,
            _config: config,
        })
    }
}
impl Drop for PendingSnapshot {
    fn drop(&mut self) {
        self.enabled.store(false, Ordering::Release);
        SNAPSHOT_PENDING.store(false, Ordering::Relaxed);
    }
}
fn copy_screenshot(image: &CGImage, max_width: usize, max_height: usize) -> Option<Image> {
    let width = CGImage::width(Some(image));
    let height = CGImage::height(Some(image));
    let stride = CGImage::bytes_per_row(Some(image));
    let length = stride.checked_mul(height)?;
    if width == 0
        || height == 0
        || width > max_width
        || height > max_height
        || CGImage::bits_per_component(Some(image)) != 8
        || CGImage::bits_per_pixel(Some(image)) != 32
        || stride < width.checked_mul(4)?
        || length > 16_000_000
    {
        return None;
    }
    let original = CGImage::data_provider(Some(image))?;
    let data = CGDataProvider::data(Some(&original))?;
    if data.length() < length as isize || data.byte_ptr().is_null() {
        return None;
    }
    // SAFETY: the checked length belongs to retained CFData. CFData::new copies
    // pixels, so neither the cached image nor AppKit retains a native surface.
    let copied = unsafe { CFData::new(None, data.byte_ptr(), length as isize)? };
    let provider = CGDataProvider::with_cf_data(Some(&copied))?;
    let color = CGImage::color_space(Some(image))?;
    let copy = unsafe {
        CGImage::new(
            width,
            height,
            8,
            32,
            stride,
            Some(&color),
            CGImage::bitmap_info(Some(image)),
            Some(&provider),
            std::ptr::null(),
            true,
            CGColorRenderingIntent::RenderingIntentDefault,
        )?
    };
    Some(Image(CFRetained::into_raw(copy).as_ptr().cast()))
}
struct Session {
    id: u32,
    width: usize,
    height: usize,
    stream: Retained<SCStream>,
    output: Retained<Output>,
    slot: Arc<Mutex<FrameSlot>>,
    running: bool,
    poisoned: bool,
    failure: Option<String>,
}
impl Session {
    fn new(r: &Request) -> Result<Self, String> {
        let filter = filter(r.id)?;
        let slot = Arc::new(Mutex::new(FrameSlot::default()));
        let output = Output::new(slot.clone());
        let config = configuration(r);
        let stream = unsafe {
            let stream = SCStream::initWithFilter_configuration_delegate(
                SCStream::alloc(),
                &filter.0,
                &config,
                Some(ProtocolObject::from_ref(&*output)),
            );
            let queue = DispatchQueue::new("io.sharif.taskbarrust.frames", None);
            stream
                .addStreamOutput_type_sampleHandlerQueue_error(
                    ProtocolObject::from_ref(&*output),
                    SCStreamOutputType::Screen,
                    Some(&queue),
                )
                .map_err(|e| e.localizedDescription().to_string())?;
            stream
        };
        SESSIONS.fetch_add(1, Ordering::Relaxed);
        LIVE.fetch_add(1, Ordering::Relaxed);
        Ok(Self {
            id: r.id,
            width: r.width,
            height: r.height,
            stream,
            output,
            slot,
            running: false,
            poisoned: false,
            failure: None,
        })
    }
    fn start(&mut self) -> Result<(), String> {
        if self.poisoned {
            return Err(self.stalled_error());
        }
        {
            let mut slot = self.slot.lock().unwrap_or_else(|e| e.into_inner());
            slot.error = None;
            slot.image = None;
            slot.enabled = true;
            slot.inactive = false;
        }
        // A timed-out start may still have reached macOS. Keep its session and
        // forbid further native calls until restart, rather than creating more.
        self.running = true;
        let result = completion(|b| unsafe {
            self.stream.startCaptureWithCompletionHandler(Some(b));
        });
        self.poisoned = result
            .as_ref()
            .is_err_and(|e| e == "Capture operation timed out");
        self.failure = result.as_ref().err().cloned();
        if result.is_err() && !self.poisoned {
            self.running = false;
            self.slot.lock().unwrap_or_else(|e| e.into_inner()).enabled = false;
        }
        result?;
        observe_native(&self.stream);
        ACTIVE.store(true, Ordering::Relaxed);
        Ok(())
    }
    fn stop(&mut self) -> Result<(), String> {
        {
            let mut slot = self.slot.lock().unwrap_or_else(|e| e.into_inner());
            if slot.error.is_some() {
                // didStopWithError is authoritative, including if it arrived
                // after a timed-out/failed native stop. Do not stop it twice or
                // permanently block later sources after their window closes.
                self.running = false;
                self.poisoned = false;
                self.failure = None;
                NATIVE_UNCERTAIN.store(false, Ordering::Relaxed);
            }
            slot.enabled = false;
            slot.image = None;
        }
        if self.poisoned {
            return Err(self.stalled_error());
        }
        if self.running {
            let result = completion(|b| unsafe {
                self.stream.stopCaptureWithCompletionHandler(Some(b));
            });
            let ended = self
                .slot
                .lock()
                .unwrap_or_else(|e| e.into_inner())
                .error
                .is_some();
            // A failed completion alone cannot establish that capture ended.
            // The delegate may confirm termination during the stop call.
            self.poisoned = result.is_err() && !ended;
            if self.poisoned {
                self.failure = result.as_ref().err().cloned();
                STOP_ERRORS.fetch_add(1, Ordering::Relaxed);
                NATIVE_UNCERTAIN.store(true, Ordering::Relaxed);
            }
            if !ended {
                result?;
            }
            self.failure = None;
            self.running = false;
            ACTIVE.store(false, Ordering::Relaxed);
            observe_native(&self.stream);
        }
        Ok(())
    }
    fn stalled_error(&self) -> String {
        format!(
            "Capture session stalled. Restart Rowla to retry. {}",
            self.failure
                .as_deref()
                .unwrap_or("Native stop is unconfirmed.")
        )
    }
    fn image(&self) -> Result<Option<Image>, String> {
        let slot = self.slot.lock().unwrap_or_else(|e| e.into_inner());
        slot.preview()
    }
}
impl Drop for Session {
    fn drop(&mut self) {
        if self.stop().is_err() {
            NATIVE_UNCERTAIN.store(true, Ordering::Relaxed);
        }
        unsafe {
            let _ = self.stream.removeStreamOutput_type_error(
                ProtocolObject::from_ref(&*self.output),
                SCStreamOutputType::Screen,
            );
        }
        LIVE.fetch_sub(1, Ordering::Relaxed);
        ACTIVE.store(false, Ordering::Relaxed);
    }
}
fn discard_obsolete<T>(
    result: Result<T, String>,
    requested: u64,
    current: u64,
) -> Result<T, String> {
    if requested != current {
        Err(CANCELLED.into())
    } else {
        result
    }
}
fn engine(rx: mpsc::Receiver<Command>) {
    let mut session: Option<Session> = None;
    let mut snapshot: Option<PendingSnapshot> = None;
    let mut listing_stalled = false;
    let mut last_request = Instant::now();
    loop {
        if SHUTTING_DOWN.load(Ordering::Acquire) {
            break;
        }
        if snapshot
            .as_ref()
            .is_some_and(|pending| !matches!(pending.rx.try_recv(), Err(mpsc::TryRecvError::Empty)))
        {
            snapshot = None;
            NATIVE_UNCERTAIN.store(false, Ordering::Relaxed);
        }
        match rx.recv_timeout(Duration::from_millis(200)) {
            Ok(Command::Shutdown(tx)) => {
                autoreleasepool(|_| {
                    session = None;
                });
                let _ = tx.send(());
                break;
            }
            Ok(Command::Capture(request)) => {
                last_request = Instant::now();
                let result = autoreleasepool(|_| {
                    if GENERATION.load(Ordering::Acquire) != request.generation {
                        return Err(CANCELLED.into());
                    }
                    // CGPreflightScreenCaptureAccess can make a synchronous
                    // native IPC call. Recheck on the worker, never in the
                    // mouse-enter handler that needs to present a cached image.
                    if !crate::platform::screen_allowed() {
                        return Err("Screen Recording permission required".into());
                    }
                    if listing_stalled {
                        return Err("Window listing stalled. Restart Rowla to retry.".into());
                    }
                    if snapshot.is_some() {
                        return Err("Screenshot completion is pending".into());
                    }
                    if request.mode == CaptureMode::Snapshot {
                        if let Some(s) = &mut session {
                            s.stop()?;
                        }
                        session = None;
                        let pending = snapshot.insert(
                            PendingSnapshot::start(&request).inspect_err(|error| {
                                listing_stalled = error == "Window listing timed out";
                            })?,
                        );
                        let result = pending.rx.recv_timeout(Duration::from_secs(5));
                        return match result {
                            Ok(result) => {
                                snapshot = None;
                                result.map(|image| (request.id, image))
                            }
                            Err(mpsc::RecvTimeoutError::Disconnected) => {
                                snapshot = None;
                                Err("Screenshot callback disconnected".into())
                            }
                            Err(mpsc::RecvTimeoutError::Timeout) => {
                                pending.enabled.store(false, Ordering::Release);
                                NATIVE_UNCERTAIN.store(true, Ordering::Relaxed);
                                Err("Screenshot operation timed out".into())
                            }
                        };
                    }
                    if let Some(current) = &mut session
                        && (current.id != request.id
                            || current.width != request.width
                            || current.height != request.height)
                    {
                        // Finish stopping before creating another session: never overlap capture sources.
                        current.stop()?;
                        session = None;
                    }
                    if session.is_none() {
                        match Session::new(&request) {
                            Ok(s) => session = Some(s),
                            Err(e) => {
                                listing_stalled = e == "Window listing timed out";
                                return Err(e);
                            }
                        }
                    }
                    if SHUTTING_DOWN.load(Ordering::Acquire) {
                        return Err("Preview cancelled during shutdown".into());
                    }
                    let s = session.as_mut().unwrap();
                    if s.slot
                        .lock()
                        .unwrap_or_else(|e| e.into_inner())
                        .error
                        .is_some()
                    {
                        s.stop()?;
                    }
                    if !s.running {
                        s.start()?;
                    }
                    let start = Instant::now();
                    loop {
                        if GENERATION.load(Ordering::Acquire) != request.generation {
                            return Err(CANCELLED.into());
                        }
                        if SHUTTING_DOWN.load(Ordering::Acquire) {
                            return Err("Preview cancelled during shutdown".into());
                        }
                        if let Some(image) = s.image()? {
                            return Ok((request.id, image));
                        }
                        if start.elapsed() > Duration::from_secs(5) {
                            return Err("Preview frame timed out".into());
                        }
                        std::thread::sleep(Duration::from_millis(10));
                    }
                });
                // Native listing/start callbacks can finish after cancellation.
                // Their images and errors belong to the obsolete request.
                let result = discard_obsolete(
                    result,
                    request.generation,
                    GENERATION.load(Ordering::Acquire),
                );
                if let Some(s) = &session {
                    observe_native(&s.stream);
                }
                if result.as_ref().is_err_and(|error| error == CANCELLED) {
                    // Release cancelled native buffers after a confirmed stop.
                    // An uncertain stop keeps its one session for recovery.
                    autoreleasepool(|_| {
                        if let Some(s) = &mut session
                            && s.stop().is_ok()
                        {
                            session = None;
                        }
                    });
                }
                if result.as_ref().is_err_and(|error| error == SOURCE_INACTIVE) {
                    autoreleasepool(|_| {
                        if let Some(s) = &mut session
                            && s.stop().is_ok()
                        {
                            session = None;
                        }
                    });
                }
                let _ = request.tx.send(result);
            }
            Err(mpsc::RecvTimeoutError::Timeout) => {
                if last_request.elapsed() > Duration::from_secs(2) {
                    autoreleasepool(|_| {
                        if let Some(s) = &mut session
                            && s.stop().is_ok()
                        {
                            // A confirmed idle stop can release its native buffers.
                            // Uncertain stops retain the session and recovery state.
                            session = None;
                        }
                    });
                }
            }
            Err(mpsc::RecvTimeoutError::Disconnected) => break,
        }
    }
    autoreleasepool(|_| {
        session = None;
    });
    STOPPED.store(true, Ordering::Release);
}
pub fn screenshot(id: u32, width: isize, height: isize, tx: Response, mode: CaptureMode) {
    if SHUTTING_DOWN.load(Ordering::Acquire) {
        let _ = tx.send(Err("Preview engine has shut down".into()));
        return;
    }
    let engine = ENGINE.get_or_init(|| {
        let (tx, rx) = mpsc::sync_channel(1);
        std::thread::spawn(move || engine(rx));
        tx
    });
    let r = Request {
        id,
        width: width.clamp(40, 1120) as usize,
        height: height.clamp(40, 800) as usize,
        tx,
        generation: GENERATION.load(Ordering::Acquire),
        mode,
    };
    if let Err(mpsc::TrySendError::Full(r) | mpsc::TrySendError::Disconnected(r)) =
        engine.try_send(Command::Capture(r))
        && let Command::Capture(r) = r
    {
        let _ = r.tx.send(Err("Capture worker is busy".into()));
    }
}
pub fn shutdown() {
    SHUTTING_DOWN.store(true, Ordering::Release);
    if let Some(engine) = ENGINE.get() {
        let (tx, rx) = mpsc::channel();
        let _ = engine.try_send(Command::Shutdown(tx));
        let start = Instant::now();
        while !STOPPED.load(Ordering::Acquire) && start.elapsed() < Duration::from_secs(6) {
            // The acknowledgement can precede the worker's final cleanup.
            // Wait for STOPPED rather than mistaking that race for uncertainty.
            let _ = rx.recv_timeout(Duration::from_millis(50));
            if matches!(rx.try_recv(), Err(mpsc::TryRecvError::Disconnected)) {
                std::thread::sleep(Duration::from_millis(50));
            }
        }
        if !STOPPED.load(Ordering::Acquire) {
            NATIVE_UNCERTAIN.store(true, Ordering::Relaxed);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::{cell::Cell, rc::Rc};

    fn test_image() -> CFRetained<CGImage> {
        let bytes = [0x80u8; 8 * 8 * 4];
        let data = unsafe { CFData::new(None, bytes.as_ptr(), bytes.len() as isize).unwrap() };
        let provider = CGDataProvider::with_cf_data(Some(&data)).unwrap();
        let color = CGColorSpace::new_device_rgb().unwrap();
        unsafe {
            CGImage::new(
                8,
                8,
                8,
                32,
                32,
                Some(&color),
                CGBitmapInfo(
                    CGImageAlphaInfo::PremultipliedFirst.0 | CGImageByteOrderInfo::Order32Little.0,
                ),
                Some(&provider),
                std::ptr::null(),
                false,
                CGColorRenderingIntent::RenderingIntentDefault,
            )
            .unwrap()
        }
    }
    #[test]
    fn snapshot_copy_rejects_oversize_and_owns_pixels_after_source_release() {
        let original = test_image();
        assert!(copy_screenshot(&original, 7, 8).is_none());
        assert!(copy_screenshot(&original, 8, 7).is_none());
        let copy = copy_screenshot(&original, 8, 8).unwrap();
        drop(original);
        let image = unsafe { &*copy.0.cast::<CGImage>() };
        assert_eq!(CGImage::width(Some(image)), 8);
        let provider = CGImage::data_provider(Some(image)).unwrap();
        let data = CGDataProvider::data(Some(&provider)).unwrap();
        assert_eq!(data.length(), 256);
        assert_eq!(unsafe { *data.byte_ptr() }, 0x80);
    }
    #[test]
    fn inactive_content_drops_live_frame_without_claiming_native_termination() {
        let image = test_image();
        let mut slot = FrameSlot {
            enabled: true,
            image: Some(Image(CFRetained::into_raw(image).as_ptr().cast())),
            ..FrameSlot::default()
        };
        slot.content_active(false);
        assert!(slot.image.is_none());
        assert!(slot.enabled);
        assert!(slot.error.is_none());
        assert_eq!(slot.preview().err().as_deref(), Some(SOURCE_INACTIVE));
        slot.content_active(true);
        assert!(slot.preview().unwrap().is_none());
        slot.error = Some("Native stream ended".into());
        slot.content_active(true);
        assert_eq!(slot.preview().err().as_deref(), Some("Native stream ended"));
    }

    #[test]
    fn obsolete_native_replies_release_images_and_do_not_report_old_errors() {
        struct Payload(Rc<Cell<usize>>);
        impl Drop for Payload {
            fn drop(&mut self) {
                self.0.set(self.0.get() + 1);
            }
        }
        let drops = Rc::new(Cell::new(0));
        let result = discard_obsolete(Ok(Payload(drops.clone())), 1, 2);
        assert!(matches!(result, Err(ref error) if error == CANCELLED));
        assert_eq!(drops.get(), 1);
        let result = discard_obsolete::<Payload>(Err("Native timeout".into()), 1, 2);
        assert!(matches!(result, Err(ref error) if error == CANCELLED));
        let result = discard_obsolete(Ok(Payload(drops.clone())), 2, 2);
        assert!(result.is_ok());
        assert_eq!(drops.get(), 1);
        drop(result);
        assert_eq!(drops.get(), 2);
        assert_eq!(
            discard_obsolete::<()>(Err("Native timeout".into()), 2, 2),
            Err("Native timeout".into())
        );
    }
}
