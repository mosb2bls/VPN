//! Process image checks (stale-core cleanup, pipe peer verification).

use std::path::{Path, PathBuf};

use windows_sys::Win32::Foundation::CloseHandle;
use windows_sys::Win32::System::Threading::{
    OpenProcess, PROCESS_NAME_WIN32, PROCESS_QUERY_LIMITED_INFORMATION, PROCESS_TERMINATE, QueryFullProcessImageNameW,
    TerminateProcess,
};

/// Full image path of a running process, if it can be queried.
pub fn image_path(pid: u32) -> Option<PathBuf> {
    let h = unsafe { OpenProcess(PROCESS_QUERY_LIMITED_INFORMATION, 0, pid) };
    if h.is_null() {
        return None;
    }
    let mut buf = vec![0u16; 1024];
    let mut len = buf.len() as u32;
    let ok = unsafe { QueryFullProcessImageNameW(h, PROCESS_NAME_WIN32, buf.as_mut_ptr(), &mut len) };
    unsafe { CloseHandle(h) };
    (ok != 0).then(|| PathBuf::from(String::from_utf16_lossy(&buf[..len as usize])))
}

/// Terminate `pid` only if it still runs `expected` (PID reuse safe enough
/// for a leftover-core cleanup). Returns true if it was terminated.
pub fn kill_if_image(pid: u32, expected: &Path) -> bool {
    match image_path(pid) {
        Some(p) if crate::paths::same_path(&p, expected) => {}
        _ => return false,
    }
    let h = unsafe { OpenProcess(PROCESS_TERMINATE, 0, pid) };
    if h.is_null() {
        return false;
    }
    let ok = unsafe { TerminateProcess(h, 1) };
    unsafe { CloseHandle(h) };
    ok != 0
}
