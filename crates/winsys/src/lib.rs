//! Windows OS integration that compiles without a C toolchain, so it can be
//! checked from macOS (`cargo check -p edge-winsys --target x86_64-pc-windows-msvc`).
//!
//! WinINet per-user system proxy (the "LAN settings" used by most apps and the
//! Windows Settings app): read and write with `InternetQueryOptionW` /
//! `InternetSetOptionW(INTERNET_OPTION_PER_CONNECTION_OPTION)` on the default
//! connection, then broadcast `INTERNET_OPTION_SETTINGS_CHANGED` and
//! `INTERNET_OPTION_REFRESH` so running apps pick the change up.
//! Reference: Microsoft Learn, "INTERNET_PER_CONN_OPTION_LISTW" and
//! "Option Flags" (WinINet).

pub use edge_core::winproxy::WinProxyState;

#[cfg(windows)]
mod imp {
    use super::WinProxyState;
    use windows_sys::Win32::Foundation::GlobalFree;
    use windows_sys::Win32::Networking::WinInet::*;

    fn wide(s: &str) -> Vec<u16> {
        s.encode_utf16().chain(std::iter::once(0)).collect()
    }

    /// Read a WinINet-allocated wide string and free it.
    unsafe fn take(p: *mut u16) -> String {
        if p.is_null() {
            return String::new();
        }
        let mut len = 0;
        while unsafe { *p.add(len) } != 0 {
            len += 1;
        }
        let s = String::from_utf16_lossy(unsafe { std::slice::from_raw_parts(p, len) });
        unsafe { GlobalFree(p as _) };
        s
    }

    pub fn read() -> Result<WinProxyState, String> {
        let mut opts: [INTERNET_PER_CONN_OPTIONW; 4] = Default::default();
        opts[0].dwOption = INTERNET_PER_CONN_FLAGS;
        opts[1].dwOption = INTERNET_PER_CONN_PROXY_SERVER;
        opts[2].dwOption = INTERNET_PER_CONN_PROXY_BYPASS;
        opts[3].dwOption = INTERNET_PER_CONN_AUTOCONFIG_URL;
        let mut list = INTERNET_PER_CONN_OPTION_LISTW {
            dwSize: std::mem::size_of::<INTERNET_PER_CONN_OPTION_LISTW>() as u32,
            pszConnection: std::ptr::null_mut(),
            dwOptionCount: opts.len() as u32,
            dwOptionError: 0,
            pOptions: opts.as_mut_ptr(),
        };
        let mut size = list.dwSize;
        let ok = unsafe {
            InternetQueryOptionW(
                std::ptr::null(),
                INTERNET_OPTION_PER_CONNECTION_OPTION,
                &mut list as *mut _ as *mut _,
                &mut size,
            )
        };
        if ok == 0 {
            return Err(format!("InternetQueryOptionW failed ({})", std::io::Error::last_os_error()));
        }
        unsafe {
            Ok(WinProxyState {
                flags: opts[0].Value.dwValue,
                server: take(opts[1].Value.pszValue),
                bypass: take(opts[2].Value.pszValue),
                pac_url: take(opts[3].Value.pszValue),
            })
        }
    }

    pub fn write(s: &WinProxyState) -> Result<(), String> {
        let mut server = wide(&s.server);
        let mut bypass = wide(&s.bypass);
        let mut pac = wide(&s.pac_url);
        let mut opts: [INTERNET_PER_CONN_OPTIONW; 4] = Default::default();
        opts[0].dwOption = INTERNET_PER_CONN_FLAGS;
        opts[0].Value.dwValue = s.flags;
        opts[1].dwOption = INTERNET_PER_CONN_PROXY_SERVER;
        opts[1].Value.pszValue = server.as_mut_ptr();
        opts[2].dwOption = INTERNET_PER_CONN_PROXY_BYPASS;
        opts[2].Value.pszValue = bypass.as_mut_ptr();
        opts[3].dwOption = INTERNET_PER_CONN_AUTOCONFIG_URL;
        opts[3].Value.pszValue = pac.as_mut_ptr();
        let list = INTERNET_PER_CONN_OPTION_LISTW {
            dwSize: std::mem::size_of::<INTERNET_PER_CONN_OPTION_LISTW>() as u32,
            pszConnection: std::ptr::null_mut(),
            dwOptionCount: opts.len() as u32,
            dwOptionError: 0,
            pOptions: opts.as_mut_ptr(),
        };
        let ok = unsafe {
            InternetSetOptionW(std::ptr::null(), INTERNET_OPTION_PER_CONNECTION_OPTION, &list as *const _ as *const _, list.dwSize)
        };
        if ok == 0 {
            return Err(format!("InternetSetOptionW failed ({})", std::io::Error::last_os_error()));
        }
        unsafe {
            InternetSetOptionW(std::ptr::null(), INTERNET_OPTION_SETTINGS_CHANGED, std::ptr::null(), 0);
            InternetSetOptionW(std::ptr::null(), INTERNET_OPTION_REFRESH, std::ptr::null(), 0);
        }
        Ok(())
    }
}

/// Current WinINet proxy settings of the default (LAN) connection.
pub fn read_proxy() -> Result<WinProxyState, String> {
    #[cfg(windows)]
    {
        imp::read()
    }
    #[cfg(not(windows))]
    {
        Err("Windows only".into())
    }
}

/// Write the settings and notify running applications.
pub fn write_proxy(s: &WinProxyState) -> Result<(), String> {
    #[cfg(windows)]
    {
        imp::write(s)
    }
    #[cfg(not(windows))]
    {
        let _ = s;
        Err("Windows only".into())
    }
}
