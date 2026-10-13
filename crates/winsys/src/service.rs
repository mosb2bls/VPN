//! Service Control Manager: install / remove the helper service and run the
//! service dispatcher (LocalSystem, automatic start, restart on failure).

use std::ffi::c_void;
use std::sync::OnceLock;
use std::sync::atomic::{AtomicPtr, Ordering};
use std::time::{Duration, Instant};

use windows_sys::Win32::Foundation::{ERROR_SERVICE_DOES_NOT_EXIST, GetLastError, NO_ERROR};
use windows_sys::Win32::System::Services::*;
use windows_sys::core::PWSTR;

use crate::wide;

/// Standard `DELETE` access right.
const DELETE: u32 = 0x0001_0000;

fn os_err(what: &str) -> String {
    format!("{what} failed ({})", std::io::Error::last_os_error())
}

struct Handle(SC_HANDLE);
impl Drop for Handle {
    fn drop(&mut self) {
        if !self.0.is_null() {
            unsafe { CloseServiceHandle(self.0) };
        }
    }
}

fn scm(access: u32) -> Result<Handle, String> {
    let h = unsafe { OpenSCManagerW(std::ptr::null(), std::ptr::null(), access) };
    if h.is_null() { Err(os_err("OpenSCManagerW")) } else { Ok(Handle(h)) }
}

pub fn is_installed(name: &str) -> bool {
    let Ok(m) = scm(SC_MANAGER_CONNECT) else { return false };
    let n = wide(name);
    let h = Handle(unsafe { OpenServiceW(m.0, n.as_ptr(), SERVICE_QUERY_STATUS) });
    !h.0.is_null()
}

/// Create (LocalSystem, auto start, restart on failure) and start the service.
pub fn install(name: &str, display: &str, description: &str, command_line: &str) -> Result<(), String> {
    let m = scm(SC_MANAGER_ALL_ACCESS)?;
    let (n, d, c) = (wide(name), wide(display), wide(command_line));
    let svc = Handle(unsafe {
        CreateServiceW(
            m.0,
            n.as_ptr(),
            d.as_ptr(),
            SERVICE_ALL_ACCESS,
            SERVICE_WIN32_OWN_PROCESS,
            SERVICE_AUTO_START,
            SERVICE_ERROR_NORMAL,
            c.as_ptr(),
            std::ptr::null(),
            std::ptr::null_mut(),
            std::ptr::null(),
            std::ptr::null(), // LocalSystem
            std::ptr::null(),
        )
    });
    if svc.0.is_null() {
        return Err(os_err("CreateServiceW"));
    }
    let mut desc = wide(description);
    let info = SERVICE_DESCRIPTIONW { lpDescription: desc.as_mut_ptr() };
    unsafe { ChangeServiceConfig2W(svc.0, SERVICE_CONFIG_DESCRIPTION, &info as *const _ as *const c_void) };
    let mut actions = [SC_ACTION { Type: SC_ACTION_RESTART, Delay: 5000 }; 3];
    let fa = SERVICE_FAILURE_ACTIONSW {
        dwResetPeriod: 24 * 3600,
        lpRebootMsg: std::ptr::null_mut(),
        lpCommand: std::ptr::null_mut(),
        cActions: actions.len() as u32,
        lpsaActions: actions.as_mut_ptr(),
    };
    if unsafe { ChangeServiceConfig2W(svc.0, SERVICE_CONFIG_FAILURE_ACTIONS, &fa as *const _ as *const c_void) } == 0 {
        return Err(os_err("ChangeServiceConfig2W(failure actions)"));
    }
    if unsafe { StartServiceW(svc.0, 0, std::ptr::null()) } == 0 {
        return Err(os_err("StartServiceW"));
    }
    Ok(())
}

/// Stop (waiting up to 30 s) and delete the service. Ok(false) if it did not exist.
pub fn remove(name: &str) -> Result<bool, String> {
    let m = scm(SC_MANAGER_CONNECT)?;
    let n = wide(name);
    let svc = Handle(unsafe { OpenServiceW(m.0, n.as_ptr(), SERVICE_STOP | SERVICE_QUERY_STATUS | DELETE) });
    if svc.0.is_null() {
        return if unsafe { GetLastError() } == ERROR_SERVICE_DOES_NOT_EXIST { Ok(false) } else { Err(os_err("OpenServiceW")) };
    }
    let mut st: SERVICE_STATUS = unsafe { std::mem::zeroed() };
    unsafe { ControlService(svc.0, SERVICE_CONTROL_STOP, &mut st) };
    let start = Instant::now();
    while start.elapsed() < Duration::from_secs(30) {
        if unsafe { QueryServiceStatus(svc.0, &mut st) } == 0 || st.dwCurrentState == SERVICE_STOPPED {
            break;
        }
        std::thread::sleep(Duration::from_millis(250));
    }
    if unsafe { DeleteService(svc.0) } == 0 {
        return Err(os_err("DeleteService"));
    }
    Ok(true)
}

/// PID of the service's process if it is running. Needs only
/// SERVICE_QUERY_STATUS, which interactive users have by default.
pub fn running_pid(name: &str) -> Option<u32> {
    let m = scm(SC_MANAGER_CONNECT).ok()?;
    let n = wide(name);
    let svc = Handle(unsafe { OpenServiceW(m.0, n.as_ptr(), SERVICE_QUERY_STATUS) });
    if svc.0.is_null() {
        return None;
    }
    let mut st: SERVICE_STATUS_PROCESS = unsafe { std::mem::zeroed() };
    let mut needed = 0u32;
    let ok = unsafe {
        QueryServiceStatusEx(
            svc.0,
            SC_STATUS_PROCESS_INFO,
            &mut st as *mut _ as *mut u8,
            std::mem::size_of::<SERVICE_STATUS_PROCESS>() as u32,
            &mut needed,
        )
    };
    (ok != 0 && st.dwCurrentState == SERVICE_RUNNING && st.dwProcessId != 0).then_some(st.dwProcessId)
}

/// The service's configured command line. Needs only SERVICE_QUERY_CONFIG
/// (interactive users have it); only administrators can change it.
pub fn command_line(name: &str) -> Option<String> {
    let m = scm(SC_MANAGER_CONNECT).ok()?;
    let n = wide(name);
    let svc = Handle(unsafe { OpenServiceW(m.0, n.as_ptr(), SERVICE_QUERY_CONFIG) });
    if svc.0.is_null() {
        return None;
    }
    let mut needed = 0u32;
    unsafe { QueryServiceConfigW(svc.0, std::ptr::null_mut(), 0, &mut needed) };
    if needed == 0 {
        return None;
    }
    let mut buf = vec![0u64; (needed as usize).div_ceil(8)];
    let cfg = buf.as_mut_ptr() as *mut QUERY_SERVICE_CONFIGW;
    if unsafe { QueryServiceConfigW(svc.0, cfg, needed, &mut needed) } == 0 {
        return None;
    }
    Some(unsafe { crate::wide_to_string((*cfg).lpBinaryPathName) })
}

// --- dispatcher -------------------------------------------------------------

static NAME: OnceLock<Vec<u16>> = OnceLock::new();
static MAIN: OnceLock<fn()> = OnceLock::new();
static ON_STOP: OnceLock<fn()> = OnceLock::new();
static STATUS: AtomicPtr<c_void> = AtomicPtr::new(std::ptr::null_mut());

fn set_status(state: SERVICE_STATUS_CURRENT_STATE, wait_hint_ms: u32) {
    let h = STATUS.load(Ordering::SeqCst);
    if h.is_null() {
        return;
    }
    let st = SERVICE_STATUS {
        dwServiceType: SERVICE_WIN32_OWN_PROCESS,
        dwCurrentState: state,
        dwControlsAccepted: if state == SERVICE_RUNNING { SERVICE_ACCEPT_STOP | SERVICE_ACCEPT_SHUTDOWN } else { 0 },
        dwWin32ExitCode: NO_ERROR,
        dwServiceSpecificExitCode: 0,
        dwCheckPoint: 0,
        dwWaitHint: wait_hint_ms,
    };
    unsafe { SetServiceStatus(h as SERVICE_STATUS_HANDLE, &st) };
}

unsafe extern "system" fn handler(control: u32, _: u32, _: *mut c_void, _: *mut c_void) -> u32 {
    match control {
        SERVICE_CONTROL_STOP | SERVICE_CONTROL_SHUTDOWN => {
            set_status(SERVICE_STOP_PENDING, 30_000);
            if let Some(f) = ON_STOP.get() {
                f();
            }
            NO_ERROR
        }
        SERVICE_CONTROL_INTERROGATE => NO_ERROR,
        _ => 120, // ERROR_CALL_NOT_IMPLEMENTED
    }
}

unsafe extern "system" fn service_main(_argc: u32, _argv: *mut PWSTR) {
    let name = NAME.get().expect("name set before dispatch");
    let h = unsafe { RegisterServiceCtrlHandlerExW(name.as_ptr(), Some(handler), std::ptr::null_mut()) };
    if h.is_null() {
        return;
    }
    STATUS.store(h, Ordering::SeqCst);
    set_status(SERVICE_RUNNING, 0);
    if let Some(f) = MAIN.get() {
        f();
    }
    set_status(SERVICE_STOPPED, 0);
}

/// Hand this process to the SCM. `main` runs while the service is running and
/// must return after `on_stop` has been called. Blocks until stopped.
pub fn run(name: &str, main: fn(), on_stop: fn()) -> Result<(), String> {
    let _ = NAME.set(wide(name));
    let _ = MAIN.set(main);
    let _ = ON_STOP.set(on_stop);
    let table = [
        SERVICE_TABLE_ENTRYW { lpServiceName: NAME.get().expect("set").as_ptr() as PWSTR, lpServiceProc: Some(service_main) },
        SERVICE_TABLE_ENTRYW { lpServiceName: std::ptr::null_mut(), lpServiceProc: None },
    ];
    if unsafe { StartServiceCtrlDispatcherW(table.as_ptr()) } == 0 {
        return Err(os_err("StartServiceCtrlDispatcherW (run by the Service Control Manager only)"));
    }
    Ok(())
}
