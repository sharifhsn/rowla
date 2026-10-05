//! Optional private APIs. No hard link to SkyLight or private AX symbols.
//! Handles stay loaded for the process lifetime because cached function pointers
//! must never outlive their image. Missing symbols have public-API fallbacks.
use std::{ffi::c_void, sync::OnceLock};
type Ref = *const c_void;
type WindowId = unsafe extern "C" fn(Ref, *mut u32) -> i32;
type Connection = unsafe extern "C" fn() -> i32;
type Spaces = unsafe extern "C" fn(i32, i32, Ref) -> Ref;
type Displays = unsafe extern "C" fn(i32) -> Ref;
struct Api {
    window_id: Option<WindowId>,
    connection: Option<Connection>,
    spaces: Option<Spaces>,
    displays: Option<Displays>,
}
static API: OnceLock<Api> = OnceLock::new();
fn api() -> &'static Api {
    API.get_or_init(|| {
        if std::env::var_os("TASKBAR_DISABLE_PRIVATE_APIS").is_some() {
            return Api {
                window_id: None,
                connection: None,
                spaces: None,
                displays: None,
            };
        }
        unsafe {
            let ax = libc::dlsym(libc::RTLD_DEFAULT, c"_AXUIElementGetWindow".as_ptr());
            let image = libc::dlopen(
                c"/System/Library/PrivateFrameworks/SkyLight.framework/SkyLight".as_ptr(),
                libc::RTLD_LAZY | libc::RTLD_LOCAL,
            );
            let symbol = |name: &std::ffi::CStr| {
                if image.is_null() {
                    std::ptr::null_mut()
                } else {
                    libc::dlsym(image, name.as_ptr())
                }
            };
            let connection = symbol(c"SLSMainConnectionID");
            let spaces = symbol(c"SLSCopySpacesForWindows");
            let displays = symbol(c"SLSCopyManagedDisplaySpaces");
            Api {
                window_id: (!ax.is_null())
                    .then(|| std::mem::transmute::<*mut c_void, WindowId>(ax)),
                connection: (!connection.is_null())
                    .then(|| std::mem::transmute::<*mut c_void, Connection>(connection)),
                spaces: (!spaces.is_null())
                    .then(|| std::mem::transmute::<*mut c_void, Spaces>(spaces)),
                displays: (!displays.is_null())
                    .then(|| std::mem::transmute::<*mut c_void, Displays>(displays)),
            }
        }
    })
}
pub(crate) fn capabilities() -> crate::models::Capabilities {
    let api = api();
    crate::models::Capabilities {
        private_window_ids: api.window_id.is_some(),
        private_spaces: api.connection.is_some() && api.spaces.is_some() && api.displays.is_some(),
    }
}
pub(crate) unsafe fn window_id(element: Ref) -> Option<u32> {
    let mut id = 0;
    let f = api().window_id?;
    (unsafe { f(element, &mut id) } == 0 && id != 0).then_some(id)
}
pub(crate) unsafe fn active_spaces() -> Ref {
    let api = api();
    match (api.connection, api.displays) {
        (Some(c), Some(f)) => unsafe { f(c()) },
        _ => std::ptr::null(),
    }
}
pub(crate) unsafe fn window_spaces(windows: Ref) -> Ref {
    let api = api();
    match (api.connection, api.spaces) {
        (Some(c), Some(f)) => unsafe { f(c(), 7, windows) },
        _ => std::ptr::null(),
    }
}
