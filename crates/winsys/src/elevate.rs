//! Run a program elevated through the standard UAC prompt
//! (`ShellExecuteExW`, verb `runas`) and wait for its exit code.

use std::path::Path;

use windows_sys::Win32::Foundation::{CloseHandle, ERROR_CANCELLED};
use windows_sys::Win32::System::Com::{COINIT_APARTMENTTHREADED, COINIT_DISABLE_OLE1DDE, CoInitializeEx};
use windows_sys::Win32::System::Threading::{GetExitCodeProcess, INFINITE, WaitForSingleObject};
use windows_sys::Win32::UI::Shell::{SEE_MASK_NOCLOSEPROCESS, SHELLEXECUTEINFOW, ShellExecuteExW};
use windows_sys::Win32::UI::WindowsAndMessaging::SW_HIDE;

use crate::{quote_arg, wide};

#[derive(Debug)]
pub enum ElevateError {
    /// The user declined the UAC prompt.
    Cancelled,
    Failed(String),
}

pub fn run_elevated(exe: &Path, args: &[String]) -> Result<u32, ElevateError> {
    // ShellExecuteEx may delegate to shell extensions, which need COM.
    unsafe { CoInitializeEx(std::ptr::null(), (COINIT_APARTMENTTHREADED | COINIT_DISABLE_OLE1DDE) as u32) };
    let verb = wide("runas");
    let file = wide(&exe.to_string_lossy());
    let params = wide(&args.iter().map(|a| quote_arg(a)).collect::<Vec<_>>().join(" "));
    let mut info: SHELLEXECUTEINFOW = unsafe { std::mem::zeroed() };
    info.cbSize = std::mem::size_of::<SHELLEXECUTEINFOW>() as u32;
    info.fMask = SEE_MASK_NOCLOSEPROCESS;
    info.lpVerb = verb.as_ptr();
    info.lpFile = file.as_ptr();
    info.lpParameters = params.as_ptr();
    info.nShow = SW_HIDE;
    if unsafe { ShellExecuteExW(&mut info) } == 0 {
        let e = std::io::Error::last_os_error();
        return Err(if e.raw_os_error() == Some(ERROR_CANCELLED as i32) {
            ElevateError::Cancelled
        } else {
            ElevateError::Failed(format!("ShellExecuteExW failed ({e})"))
        });
    }
    if info.hProcess.is_null() {
        return Err(ElevateError::Failed("no process handle".into()));
    }
    let mut code = 0u32;
    unsafe {
        WaitForSingleObject(info.hProcess, INFINITE);
        GetExitCodeProcess(info.hProcess, &mut code);
        CloseHandle(info.hProcess);
    }
    Ok(code)
}
