//! SIDs and DACLs.

use std::path::Path;

use windows_sys::Win32::Foundation::{CloseHandle, HANDLE, LocalFree};
use windows_sys::Win32::Security::Authorization::{
    ConvertSidToStringSidW, ConvertStringSecurityDescriptorToSecurityDescriptorW, SDDL_REVISION_1, SE_FILE_OBJECT,
    SetNamedSecurityInfoW,
};
use windows_sys::Win32::Security::{
    ACL, DACL_SECURITY_INFORMATION, GetSecurityDescriptorDacl, GetTokenInformation, PROTECTED_DACL_SECURITY_INFORMATION,
    PSECURITY_DESCRIPTOR, TOKEN_QUERY, TOKEN_USER, TokenUser,
};
use windows_sys::Win32::System::Threading::{GetCurrentProcess, OpenProcess, OpenProcessToken, PROCESS_QUERY_LIMITED_INFORMATION};

use crate::wide;

/// LocalSystem.
pub const SYSTEM_SID: &str = "S-1-5-18";

fn os_err(what: &str) -> String {
    format!("{what} failed ({})", std::io::Error::last_os_error())
}

unsafe fn token_user_sid(token: HANDLE) -> Result<String, String> {
    let mut len = 0u32;
    unsafe { GetTokenInformation(token, TokenUser, std::ptr::null_mut(), 0, &mut len) };
    if len == 0 {
        return Err(os_err("GetTokenInformation"));
    }
    let mut buf = vec![0u64; (len as usize).div_ceil(8)];
    if unsafe { GetTokenInformation(token, TokenUser, buf.as_mut_ptr() as _, len, &mut len) } == 0 {
        return Err(os_err("GetTokenInformation"));
    }
    let tu = buf.as_ptr() as *const TOKEN_USER;
    let mut s: *mut u16 = std::ptr::null_mut();
    if unsafe { ConvertSidToStringSidW((*tu).User.Sid, &mut s) } == 0 {
        return Err(os_err("ConvertSidToStringSidW"));
    }
    let out = unsafe { crate::wide_to_string(s) };
    unsafe { LocalFree(s as _) };
    Ok(out)
}

/// String SID of the user running this process.
pub fn current_user_sid() -> Result<String, String> {
    let mut token: HANDLE = std::ptr::null_mut();
    if unsafe { OpenProcessToken(GetCurrentProcess(), TOKEN_QUERY, &mut token) } == 0 {
        return Err(os_err("OpenProcessToken"));
    }
    let r = unsafe { token_user_sid(token) };
    unsafe { CloseHandle(token) };
    r
}

/// String SID of the user running process `pid`.
pub fn process_user_sid(pid: u32) -> Result<String, String> {
    let h = unsafe { OpenProcess(PROCESS_QUERY_LIMITED_INFORMATION, 0, pid) };
    if h.is_null() {
        return Err(os_err("OpenProcess"));
    }
    let mut token: HANDLE = std::ptr::null_mut();
    let ok = unsafe { OpenProcessToken(h, TOKEN_QUERY, &mut token) };
    unsafe { CloseHandle(h) };
    if ok == 0 {
        return Err(os_err("OpenProcessToken"));
    }
    let r = unsafe { token_user_sid(token) };
    unsafe { CloseHandle(token) };
    r
}

/// Owned security descriptor parsed from SDDL (freed on drop).
pub struct SecurityDescriptor(pub(crate) PSECURITY_DESCRIPTOR);

impl SecurityDescriptor {
    pub fn from_sddl(sddl: &str) -> Result<Self, String> {
        let w = wide(sddl);
        let mut sd: PSECURITY_DESCRIPTOR = std::ptr::null_mut();
        if unsafe { ConvertStringSecurityDescriptorToSecurityDescriptorW(w.as_ptr(), SDDL_REVISION_1, &mut sd, std::ptr::null_mut()) } == 0 {
            return Err(os_err("ConvertStringSecurityDescriptorToSecurityDescriptorW"));
        }
        Ok(Self(sd))
    }
}

impl Drop for SecurityDescriptor {
    fn drop(&mut self) {
        unsafe { LocalFree(self.0 as _) };
    }
}

/// Replace a directory's DACL with SYSTEM + Administrators full control,
/// inherited by children, not inheriting from the parent (`%ProgramData%`
/// lets Users create files in subfolders by default).
pub fn protect_dir(path: &Path) -> Result<(), String> {
    let sd = SecurityDescriptor::from_sddl("D:P(A;OICI;FA;;;SY)(A;OICI;FA;;;BA)")?;
    let (mut present, mut defaulted) = (0, 0);
    let mut dacl: *mut ACL = std::ptr::null_mut();
    if unsafe { GetSecurityDescriptorDacl(sd.0, &mut present, &mut dacl, &mut defaulted) } == 0 {
        return Err(os_err("GetSecurityDescriptorDacl"));
    }
    let p = wide(&path.to_string_lossy());
    let rc = unsafe {
        SetNamedSecurityInfoW(
            p.as_ptr(),
            SE_FILE_OBJECT,
            DACL_SECURITY_INFORMATION | PROTECTED_DACL_SECURITY_INFORMATION,
            std::ptr::null_mut(),
            std::ptr::null_mut(),
            dacl,
            std::ptr::null(),
        )
    };
    if rc != 0 {
        return Err(format!("SetNamedSecurityInfoW failed ({})", std::io::Error::from_raw_os_error(rc as i32)));
    }
    Ok(())
}
