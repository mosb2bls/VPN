//! Shell facts for the tray icon: small-icon size at the current DPI, taskbar
//! theme (light → black icon, dark → white icon) and the "show animations"
//! setting (off → no connecting animation).

use windows_sys::Win32::System::Registry::{HKEY_CURRENT_USER, RRF_RT_REG_DWORD, RegGetValueW};
use windows_sys::Win32::UI::HiDpi::{GetDpiForSystem, GetSystemMetricsForDpi};
use windows_sys::Win32::UI::WindowsAndMessaging::{SM_CXSMICON, SPI_GETCLIENTAREAANIMATION, SystemParametersInfoW};

use crate::wide;

/// Tray (small) icon size in pixels: 16 at 100 %, 20 at 125 %, 24 at 150 %, 32 at 200 %.
pub fn tray_icon_px() -> u32 {
    let px = unsafe { GetSystemMetricsForDpi(SM_CXSMICON, GetDpiForSystem()) };
    if px > 0 { px as u32 } else { 16 }
}

/// True if the taskbar / tray uses the light theme (`SystemUsesLightTheme`).
/// Missing value = dark (Windows 11 default for the taskbar).
pub fn taskbar_is_light() -> bool {
    let (k, v) = (wide(r"Software\Microsoft\Windows\CurrentVersion\Themes\Personalize"), wide("SystemUsesLightTheme"));
    let mut data = 0u32;
    let mut len = 4u32;
    let rc = unsafe {
        RegGetValueW(HKEY_CURRENT_USER, k.as_ptr(), v.as_ptr(), RRF_RT_REG_DWORD, std::ptr::null_mut(), &mut data as *mut _ as _, &mut len)
    };
    rc == 0 && data != 0
}

/// True if the user turned animations off (Settings → Accessibility → Visual effects).
pub fn reduced_motion() -> bool {
    let mut enabled = 1i32;
    let ok = unsafe { SystemParametersInfoW(SPI_GETCLIENTAREAANIMATION, 0, &mut enabled as *mut _ as _, 0) };
    ok != 0 && enabled == 0
}
