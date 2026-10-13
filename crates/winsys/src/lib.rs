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
pub mod elevate;
#[cfg(windows)]
pub mod paths;
#[cfg(windows)]
pub mod pipe;
#[cfg(windows)]
pub mod process;
#[cfg(windows)]
pub mod security;
#[cfg(windows)]
pub mod service;
#[cfg(windows)]
pub mod startup;
#[cfg(windows)]
pub mod wfp;

/// Read a NUL-terminated wide string (not freed).
#[cfg(windows)]
pub(crate) unsafe fn wide_to_string(p: *const u16) -> String {
    if p.is_null() {
        return String::new();
    }
    let mut len = 0;
    while unsafe { *p.add(len) } != 0 {
        len += 1;
    }
    String::from_utf16_lossy(unsafe { std::slice::from_raw_parts(p, len) })
}

/// NUL-terminated UTF-16.
#[cfg(windows)]
pub(crate) fn wide(s: &str) -> Vec<u16> {
    s.encode_utf16().chain(std::iter::once(0)).collect()
}

/// Helper pipe access rights (ADR-0007). The pipe DACL grants the user exactly
/// `PIPE_USER_GRANT`: FILE_GENERIC_READ (READ_DATA | READ_EA | READ_ATTRIBUTES |
/// READ_CONTROL | SYNCHRONIZE) + FILE_WRITE_DATA — deliberately **not**
/// FILE_APPEND_DATA (= FILE_CREATE_PIPE_INSTANCE on a pipe), WRITE_EA or
/// WRITE_ATTRIBUTES.
pub const PIPE_USER_GRANT: u32 = 0x0012_008B;
/// What the client asks for when opening the pipe: READ_DATA | WRITE_DATA |
/// READ_ATTRIBUTES (GetNamedPipeServerProcessId) | SYNCHRONIZE. Must be a subset of
/// `PIPE_USER_GRANT` — `GENERIC_WRITE` is not (it includes APPEND_DATA, WRITE_EA,
/// WRITE_ATTRIBUTES) and was refused with "Access is denied" (Windows round 1, F-1).
pub const PIPE_CLIENT_ACCESS: u32 = 0x0000_0001 | 0x0000_0002 | 0x0000_0080 | 0x0010_0000;

/// Quote one argument so `CommandLineToArgvW` / the MSVC runtime parse it
/// back unchanged (backslashes are literal except before a quote).
pub fn quote_arg(arg: &str) -> String {
    if !arg.is_empty() && !arg.contains([' ', '\t', '\n', '\x0b', '"']) {
        return arg.to_string();
    }
    let mut out = String::from('"');
    let mut backslashes = 0;
    for c in arg.chars() {
        match c {
            '\\' => backslashes += 1,
            '"' => {
                out.extend(std::iter::repeat_n('\\', backslashes * 2 + 1));
                out.push('"');
                backslashes = 0;
            }
            _ => {
                out.extend(std::iter::repeat_n('\\', backslashes));
                out.push(c);
                backslashes = 0;
            }
        }
    }
    out.extend(std::iter::repeat_n('\\', backslashes * 2));
    out.push('"');
    out
}

/// A Windows string SID (`S-1-5-21-…`): digits and dashes only, so it is safe
/// to embed in an SDDL string.
pub fn valid_sid(s: &str) -> bool {
    s.len() <= 184
        && s.strip_prefix("S-1-").is_some_and(|rest| !rest.is_empty() && rest.split('-').all(|p| !p.is_empty() && p.bytes().all(|b| b.is_ascii_digit())))
}


#[cfg(windows)]
mod imp {
    use super::WinProxyState;
    use windows_sys::Win32::Foundation::GlobalFree;
    use windows_sys::Win32::Networking::WinInet::*;

    use crate::wide;

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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn quoting_round_trips_msvc_rules() {
        assert_eq!(quote_arg("install"), "install");
        assert_eq!(quote_arg(r"C:\Program Files\x.exe"), r#""C:\Program Files\x.exe""#);
        assert_eq!(quote_arg(r"C:\dir with space\"), r#""C:\dir with space\\""#);
        assert_eq!(quote_arg(r#"a"b"#), r#""a\"b""#);
        assert_eq!(quote_arg(""), r#""""#);
    }

    #[test]
    fn pipe_client_access_is_within_the_grant() {
        assert_eq!(PIPE_CLIENT_ACCESS & !PIPE_USER_GRANT, 0, "client asks for rights the DACL does not grant");
        assert_eq!(PIPE_USER_GRANT & 0x4, 0, "FILE_APPEND_DATA / CREATE_PIPE_INSTANCE must not be granted");
        // GENERIC_WRITE's file mapping (FILE_GENERIC_WRITE) is NOT within the grant.
        const FILE_GENERIC_WRITE: u32 = 0x0012_0116;
        assert_ne!(FILE_GENERIC_WRITE & !PIPE_USER_GRANT, 0);
    }

    #[test]
    fn sid_validation() {
        assert!(valid_sid("S-1-5-21-1004336348-1177238915-682003330-1001"));
        assert!(valid_sid("S-1-5-18"));
        for bad in ["", "S-1-", "S-1-5-", "S-1-5-x", "S-1-5-21)(A;;GA;;;WD)", "s-1-5-18", "S-1-5--1"] {
            assert!(!valid_sid(bad), "{bad}");
        }
    }
}
