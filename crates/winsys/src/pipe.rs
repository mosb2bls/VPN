//! Named pipe between the app (client) and the helper service (server).
//!
//! Server: explicit DACL from SDDL (never the default, which lets Everyone
//! read), `FILE_FLAG_FIRST_PIPE_INSTANCE` on the first instance so a process
//! that pre-created the name makes the service fail instead of being bypassed,
//! `PIPE_REJECT_REMOTE_CLIENTS`. Client: verifies the server process image
//! before sending anything (Microsoft Learn, CreateNamedPipeW).

use std::fs::File;
use std::os::windows::io::{AsRawHandle, FromRawHandle};
use std::time::{Duration, Instant};

use windows_sys::Win32::Foundation::{CloseHandle, ERROR_PIPE_BUSY, ERROR_PIPE_CONNECTED, GetLastError, HANDLE, INVALID_HANDLE_VALUE};
use windows_sys::Win32::Security::SECURITY_ATTRIBUTES;
use windows_sys::Win32::Storage::FileSystem::{FILE_FLAG_FIRST_PIPE_INSTANCE, FlushFileBuffers, PIPE_ACCESS_DUPLEX};
use windows_sys::Win32::System::Pipes::{
    ConnectNamedPipe, CreateNamedPipeW, DisconnectNamedPipe, GetNamedPipeClientProcessId, GetNamedPipeServerProcessId,
    PIPE_READMODE_BYTE, PIPE_REJECT_REMOTE_CLIENTS, PIPE_TYPE_BYTE, PIPE_UNLIMITED_INSTANCES, PIPE_WAIT, WaitNamedPipeW,
};

use crate::security::SecurityDescriptor;
use crate::wide;

pub struct Listener {
    name: Vec<u16>,
    sd: SecurityDescriptor,
    first: bool,
}

impl Listener {
    pub fn new(name: &str, sddl: &str) -> Result<Self, String> {
        Ok(Self { name: wide(name), sd: SecurityDescriptor::from_sddl(sddl)?, first: true })
    }

    /// Create the next pipe instance and block until a client connects.
    pub fn accept(&mut self) -> Result<Conn, String> {
        let sa = SECURITY_ATTRIBUTES {
            nLength: std::mem::size_of::<SECURITY_ATTRIBUTES>() as u32,
            lpSecurityDescriptor: self.sd.0,
            bInheritHandle: 0,
        };
        let mode = PIPE_ACCESS_DUPLEX | if self.first { FILE_FLAG_FIRST_PIPE_INSTANCE } else { 0 };
        let h = unsafe {
            CreateNamedPipeW(
                self.name.as_ptr(),
                mode,
                PIPE_TYPE_BYTE | PIPE_READMODE_BYTE | PIPE_WAIT | PIPE_REJECT_REMOTE_CLIENTS,
                PIPE_UNLIMITED_INSTANCES,
                64 * 1024,
                64 * 1024,
                0,
                &sa,
            )
        };
        if h == INVALID_HANDLE_VALUE {
            return Err(format!("CreateNamedPipeW failed ({})", std::io::Error::last_os_error()));
        }
        self.first = false;
        let ok = unsafe { ConnectNamedPipe(h, std::ptr::null_mut()) };
        if ok == 0 && unsafe { GetLastError() } != ERROR_PIPE_CONNECTED {
            let e = std::io::Error::last_os_error();
            unsafe { CloseHandle(h) };
            return Err(format!("ConnectNamedPipe failed ({e})"));
        }
        Ok(Conn { file: unsafe { File::from_raw_handle(h as _) } })
    }
}

/// One connected client. Flushed and disconnected on drop.
pub struct Conn {
    file: File,
}

impl Conn {
    pub fn file(&self) -> &File {
        &self.file
    }

    pub fn client_pid(&self) -> Option<u32> {
        let mut pid = 0u32;
        let ok = unsafe { GetNamedPipeClientProcessId(self.file.as_raw_handle() as HANDLE, &mut pid) };
        (ok != 0).then_some(pid)
    }
}

impl Drop for Conn {
    fn drop(&mut self) {
        let h = self.file.as_raw_handle() as HANDLE;
        unsafe {
            FlushFileBuffers(h);
            DisconnectNamedPipe(h);
        }
    }
}

/// Open the pipe as a client, waiting up to `wait` while all instances are busy.
pub fn connect(name: &str, wait: Duration) -> std::io::Result<File> {
    let start = Instant::now();
    loop {
        match std::fs::OpenOptions::new().read(true).write(true).open(name) {
            Ok(f) => return Ok(f),
            Err(e) if e.raw_os_error() == Some(ERROR_PIPE_BUSY as i32) && start.elapsed() < wait => {
                let w = wide(name);
                unsafe { WaitNamedPipeW(w.as_ptr(), 500) };
            }
            Err(e) => return Err(e),
        }
    }
}

/// PID of the process serving a pipe the client opened.
pub fn server_pid(f: &File) -> Option<u32> {
    let mut pid = 0u32;
    let ok = unsafe { GetNamedPipeServerProcessId(f.as_raw_handle() as HANDLE, &mut pid) };
    (ok != 0).then_some(pid)
}
