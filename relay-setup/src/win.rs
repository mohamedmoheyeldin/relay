//! The Windows calls Relay Setup makes: read the uninstall keys, find and close a
//! running Relay, and run a program elevated (one UAC prompt) and wait for it.

use crate::state::{self, Installed, Scope, UNINSTALL_KEY};
use std::ffi::{c_void, OsStr};
use std::os::windows::ffi::OsStrExt;
use std::path::{Path, PathBuf};
use std::ptr::{null, null_mut};
use windows_sys::Win32::Foundation::{CloseHandle, GetLastError, HANDLE, INVALID_HANDLE_VALUE};
use windows_sys::Win32::System::Diagnostics::ToolHelp::{
    CreateToolhelp32Snapshot, Process32FirstW, Process32NextW, PROCESSENTRY32W, TH32CS_SNAPPROCESS,
};
use windows_sys::Win32::System::Registry::{
    RegGetValueW, HKEY, HKEY_CURRENT_USER, HKEY_LOCAL_MACHINE, RRF_RT_REG_EXPAND_SZ, RRF_RT_REG_SZ,
    RRF_SUBKEY_WOW6464KEY,
};
use windows_sys::Win32::System::Threading::{
    GetExitCodeProcess, OpenProcess, QueryFullProcessImageNameW, TerminateProcess,
    WaitForSingleObject, INFINITE, PROCESS_NAME_WIN32, PROCESS_QUERY_LIMITED_INFORMATION,
    PROCESS_SYNCHRONIZE, PROCESS_TERMINATE,
};
use windows_sys::Win32::UI::Shell::{
    ShellExecuteExW, SEE_MASK_FLAG_NO_UI, SEE_MASK_NOASYNC, SEE_MASK_NOCLOSEPROCESS,
    SHELLEXECUTEINFOW,
};
use windows_sys::Win32::UI::WindowsAndMessaging::{SW_HIDE, SW_SHOWNORMAL};

fn wide(text: impl AsRef<OsStr>) -> Vec<u16> {
    text.as_ref().encode_wide().chain(Some(0)).collect()
}

fn reg_string(root: HKEY, subkey: &str, name: &str) -> Option<String> {
    let subkey = wide(subkey);
    let name = wide(name);
    let flags = RRF_RT_REG_SZ | RRF_RT_REG_EXPAND_SZ | RRF_SUBKEY_WOW6464KEY;
    let mut size = 0u32;
    // SAFETY: valid NUL-terminated strings; a null buffer asks only for the size.
    let status = unsafe {
        RegGetValueW(
            root,
            subkey.as_ptr(),
            name.as_ptr(),
            flags,
            null_mut(),
            null_mut(),
            &mut size,
        )
    };
    if status != 0 || size == 0 {
        return None;
    }
    let mut buf = vec![0u16; (size as usize).div_ceil(2) + 1];
    let mut bytes = (buf.len() * 2) as u32;
    // SAFETY: `buf` holds `bytes` bytes.
    let status = unsafe {
        RegGetValueW(
            root,
            subkey.as_ptr(),
            name.as_ptr(),
            flags,
            null_mut(),
            buf.as_mut_ptr() as *mut c_void,
            &mut bytes,
        )
    };
    if status != 0 {
        return None;
    }
    let len = buf.iter().position(|&c| c == 0).unwrap_or(buf.len());
    Some(String::from_utf16_lossy(&buf[..len]))
}

fn read_install(root: HKEY, scope: Scope) -> Option<Installed> {
    let version = reg_string(root, UNINSTALL_KEY, "DisplayVersion");
    let uninstall_string = reg_string(root, UNINSTALL_KEY, "UninstallString");
    let location = reg_string(root, UNINSTALL_KEY, "InstallLocation")
        .map(|l| state::unquote(&l))
        .filter(|l| !l.is_empty())
        .or_else(|| {
            uninstall_string
                .as_deref()
                .and_then(state::uninstaller_path)
                .and_then(|p| state::parent_dir(&p))
        });
    if version.is_none() && uninstall_string.is_none() {
        return None;
    }
    Some(Installed {
        version: version.unwrap_or_default(),
        location,
        uninstall_string,
        scope,
    })
}

/// (per-machine install from HKLM, per-user 0.1.0 from HKCU)
pub fn installs() -> (Option<Installed>, Option<Installed>) {
    (
        read_install(HKEY_LOCAL_MACHINE, Scope::Machine),
        read_install(HKEY_CURRENT_USER, Scope::User),
    )
}

/// Where a fresh per-machine install goes (what the NSIS template picks).
pub fn default_location() -> String {
    let base = std::env::var("ProgramW6432")
        .or_else(|_| std::env::var("ProgramFiles"))
        .unwrap_or_else(|_| r"C:\Program Files".into());
    format!(r"{}\Relay", base.trim_end_matches('\\'))
}

struct Handle(HANDLE);

impl Drop for Handle {
    fn drop(&mut self) {
        if !self.0.is_null() && self.0 != INVALID_HANDLE_VALUE {
            // SAFETY: a handle this module opened and owns.
            unsafe { CloseHandle(self.0) };
        }
    }
}

fn image_path(pid: u32) -> Option<PathBuf> {
    // SAFETY: plain Win32 calls; the handle is closed by `Handle`.
    unsafe {
        let process = Handle(OpenProcess(PROCESS_QUERY_LIMITED_INFORMATION, 0, pid));
        if process.0.is_null() {
            return None;
        }
        let mut buf = vec![0u16; 32768];
        let mut len = buf.len() as u32;
        if QueryFullProcessImageNameW(process.0, PROCESS_NAME_WIN32, buf.as_mut_ptr(), &mut len)
            == 0
        {
            return None;
        }
        Some(PathBuf::from(String::from_utf16_lossy(
            &buf[..len as usize],
        )))
    }
}

/// A process started from one of Relay's install folders: relay.exe, or the bundled
/// Python running relay-screen.
pub struct Running {
    pub pid: u32,
    pub path: PathBuf,
}

fn is_inside(path: &Path, dir: &str) -> bool {
    let path = path.to_string_lossy().to_lowercase();
    let dir = format!("{}\\", dir.trim_end_matches('\\').to_lowercase());
    path.starts_with(&dir)
}

pub fn running_from(dirs: &[String]) -> Vec<Running> {
    let dirs: Vec<&String> = dirs.iter().filter(|d| !d.is_empty()).collect();
    if dirs.is_empty() {
        return Vec::new();
    }
    let mut found = Vec::new();
    // SAFETY: Toolhelp snapshot walk; the snapshot handle is closed by `Handle`.
    unsafe {
        let snap = Handle(CreateToolhelp32Snapshot(TH32CS_SNAPPROCESS, 0));
        if snap.0 == INVALID_HANDLE_VALUE {
            return found;
        }
        let mut entry: PROCESSENTRY32W = std::mem::zeroed();
        entry.dwSize = std::mem::size_of::<PROCESSENTRY32W>() as u32;
        let mut ok = Process32FirstW(snap.0, &mut entry);
        while ok != 0 {
            let pid = entry.th32ProcessID;
            if pid != std::process::id() && pid != 0 {
                if let Some(path) = image_path(pid) {
                    if dirs.iter().any(|d| is_inside(&path, d)) {
                        found.push(Running { pid, path });
                    }
                }
            }
            ok = Process32NextW(snap.0, &mut entry);
        }
    }
    // relay.exe first, so it can't start a new relay-screen after its Python is closed.
    found.sort_by_key(|r| {
        r.path
            .file_name()
            .map(|n| !n.eq_ignore_ascii_case("relay.exe"))
            .unwrap_or(true)
    });
    found
}

/// Close Relay and its relay-screen processes. Relay keeps running in the tray when
/// its window closes, so a polite close would not end it; nothing is lost by ending
/// it (chats are on disk). Returns how many could not be closed (for example, ones
/// running as administrator); the NSIS installer closes those through the Restart
/// Manager once it runs elevated.
pub fn close_running(dirs: &[String]) -> usize {
    let mut left = 0;
    for proc in running_from(dirs) {
        // SAFETY: plain Win32 calls; the handle is closed by `Handle`.
        let closed = unsafe {
            let h = Handle(OpenProcess(
                PROCESS_TERMINATE | PROCESS_SYNCHRONIZE,
                0,
                proc.pid,
            ));
            !h.0.is_null() && TerminateProcess(h.0, 0) != 0 && {
                WaitForSingleObject(h.0, 5000);
                true
            }
        };
        if !closed {
            left += 1;
        }
    }
    left
}

/// Run `file` with `params` through the "runas" verb (one UAC prompt), wait for it,
/// and return its exit code. `started` runs once the prompt was accepted and the
/// program started. `Err(1223)` (ERROR_CANCELLED) when the owner declines the prompt.
pub fn run_elevated(
    file: &Path,
    params: &str,
    owner: isize,
    hidden: bool,
    started: impl FnOnce(),
) -> Result<u32, u32> {
    let verb = wide("runas");
    let file = wide(file);
    let params = wide(params);
    // SAFETY: SHELLEXECUTEINFOW is plain data; every string outlives the call.
    unsafe {
        let mut info: SHELLEXECUTEINFOW = std::mem::zeroed();
        info.cbSize = std::mem::size_of::<SHELLEXECUTEINFOW>() as u32;
        info.fMask = SEE_MASK_NOCLOSEPROCESS | SEE_MASK_NOASYNC | SEE_MASK_FLAG_NO_UI;
        info.hwnd = owner as _;
        info.lpVerb = verb.as_ptr();
        info.lpFile = file.as_ptr();
        info.lpParameters = params.as_ptr();
        info.lpDirectory = null();
        info.nShow = if hidden { SW_HIDE } else { SW_SHOWNORMAL };
        if ShellExecuteExW(&mut info) == 0 {
            return Err(GetLastError());
        }
        started();
        if info.hProcess.is_null() {
            return Ok(0);
        }
        let process = Handle(info.hProcess);
        WaitForSingleObject(process.0, INFINITE);
        let mut code = 0u32;
        if GetExitCodeProcess(process.0, &mut code) == 0 {
            return Err(GetLastError());
        }
        Ok(code)
    }
}

/// `relay-setup --uninstall-elevated <uninstaller> <install dir>`: the elevated half of
/// Uninstall. NSIS uninstallers copy themselves to %TEMP% and return at once unless
/// `_?=<dir>` runs them in place, so this runs the uninstaller with `/S _?=<dir>`,
/// waits, then removes what an in-place uninstaller can't: itself and the folder.
pub fn uninstall_in_place(uninstaller: &str, dir: &str) -> u32 {
    use std::os::windows::process::CommandExt;
    const CREATE_NO_WINDOW: u32 = 0x0800_0000;
    // `_?=` must come last and must not be quoted, even with spaces in the path.
    let status = std::process::Command::new(uninstaller)
        .raw_arg("/S")
        .raw_arg(format!("_?={dir}"))
        .creation_flags(CREATE_NO_WINDOW)
        .status();
    let code = match status {
        Ok(s) => s.code().map(|c| c as u32).unwrap_or(1),
        Err(_) => return 3,
    };
    if code == 0 {
        // The uninstaller's own hook already removed `python`; the folder goes only
        // if it is now empty.
        let _ = std::fs::remove_file(uninstaller);
        let _ = std::fs::remove_dir(dir);
    }
    code
}
