//! Windows login-item approval state. Task Manager → Startup apps records
//! enable / disable per Run value in
//! `HKCU\Software\Microsoft\Windows\CurrentVersion\Explorer\StartupApproved\Run`
//! (binary; first byte even = enabled, odd = disabled). The autostart plugin
//! only manages the `Run` value itself, so the app reads and clears this record.

use windows_sys::Win32::System::Registry::{
    HKEY_CURRENT_USER, REG_BINARY, RRF_RT_REG_BINARY, RegDeleteKeyValueW, RegGetValueW, RegSetKeyValueW,
};

use crate::wide;

const KEY: &str = r"Software\Microsoft\Windows\CurrentVersion\Explorer\StartupApproved\Run";

/// True if the user disabled the login item `name` in Task Manager.
pub fn approved_disabled(name: &str) -> bool {
    let (k, v) = (wide(KEY), wide(name));
    let mut buf = [0u8; 16];
    let mut len = buf.len() as u32;
    let rc = unsafe {
        RegGetValueW(HKEY_CURRENT_USER, k.as_ptr(), v.as_ptr(), RRF_RT_REG_BINARY, std::ptr::null_mut(), buf.as_mut_ptr() as _, &mut len)
    };
    rc == 0 && len > 0 && buf[0] & 1 == 1
}

/// Mark the login item enabled (after the app turns launch at login ON): the
/// record Windows itself writes, 12 bytes = DWORD 2 + zero FILETIME. Deleting it
/// instead made Task Manager stop listing the app (round 3, F-8).
pub fn approved_set_enabled(name: &str) {
    let (k, v) = (wide(KEY), wide(name));
    let data: [u8; 12] = [2, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0];
    unsafe { RegSetKeyValueW(HKEY_CURRENT_USER, k.as_ptr(), v.as_ptr(), REG_BINARY, data.as_ptr() as _, data.len() as u32) };
}

/// Remove the approval record (after launch at login OFF and on uninstall, so
/// nothing lingers — round 1, M-2).
pub fn approved_clear(name: &str) {
    let (k, v) = (wide(KEY), wide(name));
    unsafe { RegDeleteKeyValueW(HKEY_CURRENT_USER, k.as_ptr(), v.as_ptr()) };
}
