//! Keeps Relay-Setup.exe a single file on the MinGW build.
//!
//! On `x86_64-pc-windows-gnu`, webview2-com-sys imports its two loader functions from
//! WebView2Loader.dll, which would have to sit next to the exe (MSVC builds link the
//! loader statically and don't need this). Defining the two functions here satisfies
//! those imports at link time, so the exe has no WebView2Loader.dll import; at run time
//! they load the real loader, embedded by build.rs, from a copy in %TEMP%.

use std::ffi::c_void;
use std::os::windows::ffi::OsStrExt;
use std::sync::OnceLock;
use windows_sys::Win32::System::LibraryLoader::{GetProcAddress, LoadLibraryW};

/// WebView2Loader.dll from webview2-com-sys (`RELAY_SETUP_WEBVIEW2_LOADER`, set by
/// scripts/build_installer.py). Empty in lint and test builds.
static LOADER: &[u8] = include_bytes!(concat!(env!("OUT_DIR"), "/WebView2Loader.dll"));

const E_FAIL: i32 = 0x8000_4005_u32 as i32;

fn module() -> Option<usize> {
    static MODULE: OnceLock<Option<usize>> = OnceLock::new();
    *MODULE.get_or_init(|| {
        if LOADER.is_empty() {
            return None;
        }
        let dir = std::env::temp_dir().join("Relay-Setup");
        std::fs::create_dir_all(&dir).ok()?;
        let path = dir.join(format!("WebView2Loader-{}.dll", LOADER.len()));
        let current = std::fs::read(&path).map(|b| b == LOADER).unwrap_or(false);
        if !current {
            std::fs::write(&path, LOADER).ok()?;
        }
        let wide: Vec<u16> = path.as_os_str().encode_wide().chain(Some(0)).collect();
        // SAFETY: a NUL-terminated path to the loader just written.
        let module = unsafe { LoadLibraryW(wide.as_ptr()) };
        (!module.is_null()).then_some(module as usize)
    })
}

fn proc(name: &[u8]) -> Option<unsafe extern "system" fn() -> isize> {
    let module = module()?;
    // SAFETY: `name` is NUL-terminated; `module` is a loaded library.
    unsafe { GetProcAddress(module as _, name.as_ptr()) }
}

type CreateEnvironment =
    unsafe extern "system" fn(*const u16, *const u16, *mut c_void, *mut c_void) -> i32;
type BrowserVersion = unsafe extern "system" fn(*const u16, *mut *mut u16) -> i32;

/// # Safety
/// Same contract as WebView2's `CreateCoreWebView2EnvironmentWithOptions`.
#[no_mangle]
pub unsafe extern "system" fn CreateCoreWebView2EnvironmentWithOptions(
    browser_folder: *const u16,
    user_data_folder: *const u16,
    options: *mut c_void,
    handler: *mut c_void,
) -> i32 {
    match proc(b"CreateCoreWebView2EnvironmentWithOptions\0") {
        Some(f) => {
            let f: CreateEnvironment = std::mem::transmute(f);
            f(browser_folder, user_data_folder, options, handler)
        }
        None => E_FAIL,
    }
}

/// # Safety
/// Same contract as WebView2's `GetAvailableCoreWebView2BrowserVersionString`.
#[no_mangle]
pub unsafe extern "system" fn GetAvailableCoreWebView2BrowserVersionString(
    browser_folder: *const u16,
    version: *mut *mut u16,
) -> i32 {
    match proc(b"GetAvailableCoreWebView2BrowserVersionString\0") {
        Some(f) => {
            let f: BrowserVersion = std::mem::transmute(f);
            f(browser_folder, version)
        }
        None => E_FAIL,
    }
}
