//! Install and data locations of the Windows helper service (ADR-0007).
//! Resolved with `SHGetKnownFolderPath`, not environment variables, so the
//! elevated installer, the service (LocalSystem) and the app agree.

use std::path::PathBuf;

use windows_sys::Win32::System::Com::CoTaskMemFree;
use windows_sys::Win32::UI::Shell::{FOLDERID_ProgramData, FOLDERID_ProgramFiles, SHGetKnownFolderPath};
use windows_sys::core::{GUID, PWSTR};

fn known(id: &GUID, fallback: &str) -> PathBuf {
    let mut p: PWSTR = std::ptr::null_mut();
    let hr = unsafe { SHGetKnownFolderPath(id, 0, std::ptr::null_mut(), &mut p) };
    let out = (hr == 0 && !p.is_null()).then(|| unsafe { crate::wide_to_string(p) });
    unsafe { CoTaskMemFree(p as _) };
    PathBuf::from(out.unwrap_or_else(|| fallback.into()))
}

/// `%ProgramFiles%\<App>\Helper` — admin-writable only (inherited ACL).
pub fn install_dir() -> PathBuf {
    known(&FOLDERID_ProgramFiles, r"C:\Program Files").join(edge_core::APP_NAME).join("Helper")
}

/// `%ProgramData%\<App>\Helper` — protected DACL (SYSTEM + Administrators) set at install.
pub fn data_dir() -> PathBuf {
    known(&FOLDERID_ProgramData, r"C:\ProgramData").join(edge_core::APP_NAME).join("Helper")
}

pub fn helper_exe() -> PathBuf {
    install_dir().join("edge-helper.exe")
}

pub fn singbox_exe() -> PathBuf {
    install_dir().join("sing-box.exe")
}

/// Case-insensitive path equality (Windows file system semantics).
pub fn same_path(a: &std::path::Path, b: &std::path::Path) -> bool {
    a.to_string_lossy().to_lowercase() == b.to_string_lossy().to_lowercase()
}
