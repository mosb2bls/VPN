//! Windows login-item approval state. Task Manager → Startup apps records
//! enable / disable per Run value in
//! `HKCU\Software\Microsoft\Windows\CurrentVersion\Explorer\StartupApproved\Run`
//! (binary; first byte even = enabled, odd = disabled). The autostart plugin
//! only manages the `Run` value itself, so the app reads and clears this record.

use windows_sys::Win32::System::Registry::{HKEY_CURRENT_USER, RRF_RT_REG_BINARY, RegDeleteKeyValueW, RegGetValueW};

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

/// Remove the approval record (after our own enable / disable, so it never
/// overrides the user's latest choice in the app or lingers after OFF).
pub fn approved_clear(name: &str) {
    let (k, v) = (wide(KEY), wide(name));
    unsafe { RegDeleteKeyValueW(HKEY_CURRENT_USER, k.as_ptr(), v.as_ptr()) };
}
