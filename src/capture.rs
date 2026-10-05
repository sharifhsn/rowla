//! One bounded capture session, one latest frame, and no per-frame screenshot RPC.
use crate::platform::Image;
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
    SCContentFilter, SCShareableContent, SCStream, SCStreamConfiguration, SCStreamDelegate,
    SCStreamOutput, SCStreamOutputType,
};
use std::{
    sync::{
        Arc, Mutex, OnceLock,
        atomic::{AtomicBool, AtomicU64, Ordering},
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
                    if !slot.enabled {
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
        let config = unsafe {
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
        };
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
        if let Some(error) = &slot.error {
            return Err(error.clone());
        }
        Ok(slot.image.clone())
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
    let mut listing_stalled = false;
    let mut last_request = Instant::now();
    loop {
        if SHUTTING_DOWN.load(Ordering::Acquire) {
            break;
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
                    if session.as_ref().is_some_and(|s| {
                        s.id != request.id || s.width != request.width || s.height != request.height
                    }) {
                        // Finish stopping before creating another session: never overlap capture sources.
                        session.as_mut().unwrap().stop()?;
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
pub fn screenshot(id: u32, width: isize, height: isize, tx: Response) {
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
