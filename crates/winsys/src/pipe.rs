//! Named pipe between the app (client) and the helper service (server).
//!
//! Server: explicit DACL from SDDL (never the default, which lets Everyone
//! read), `FILE_FLAG_FIRST_PIPE_INSTANCE` on the first instance so a process
//! that pre-created the name makes the service fail instead of being bypassed,
//! `PIPE_REJECT_REMOTE_CLIENTS`. Client: verifies the server process image
//! before sending anything (Microsoft Learn, CreateNamedPipeW).

use std::fs::File;
use std::os::windows::fs::OpenOptionsExt;
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
/// Asks for exactly `PIPE_CLIENT_ACCESS` (not GENERIC_READ | GENERIC_WRITE, which
/// the pipe DACL refuses — see `crate::PIPE_USER_GRANT`).
pub fn connect(name: &str, wait: Duration) -> std::io::Result<File> {
    let start = Instant::now();
    loop {
        match std::fs::OpenOptions::new().access_mode(crate::PIPE_CLIENT_ACCESS).open(name) {
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

/// Real pipe with the helper's DACL, opened by the current (non-SYSTEM) user —
/// the round-1 F-1 regression test. Runs on the Windows CI runner.
#[cfg(test)]
mod tests {
    use std::io::{BufRead, BufReader, Write};
    use std::time::Duration;

    use super::*;

    #[test]
    fn user_can_open_helper_pipe_but_not_with_generic_write() {
        let sid = crate::security::current_user_sid().expect("current user SID");
        let name = format!(r"\\.\pipe\edge-winsys-test-{}", std::process::id());
        let sddl = format!("D:P(A;;GA;;;SY)(A;;0x{:08x};;;{sid})", crate::PIPE_USER_GRANT);
        let (ready_tx, ready_rx) = std::sync::mpsc::channel();
        let server_name = name.clone();
        let server = std::thread::spawn(move || {
            // The listener (raw security descriptor) stays on this thread.
            let mut listener = Listener::new(&server_name, &sddl).expect("listener");
            let _ = ready_tx.send(());
            let conn = listener.accept().expect("accept");
            assert!(conn.client_pid().is_some());
            let mut line = String::new();
            BufReader::new(conn.file()).read_line(&mut line).unwrap();
            conn.file().write_all(format!("echo {line}").as_bytes()).unwrap();
            // Second client: same pipe, GENERIC_READ | GENERIC_WRITE (old client code).
            listener.accept().ok().map(|_| ())
        });
        ready_rx.recv().unwrap();
        std::thread::sleep(Duration::from_millis(200));
        let f = connect(&name, Duration::from_secs(5)).expect("client connect with PIPE_CLIENT_ACCESS");
        assert_eq!(server_pid(&f), Some(std::process::id()));
        (&f).write_all(b"ping\n").unwrap();
        let mut reply = String::new();
        BufReader::new(&f).read_line(&mut reply).unwrap();
        assert_eq!(reply.trim(), "echo ping");
        drop(f);
        std::thread::sleep(Duration::from_millis(200));
        let denied = std::fs::OpenOptions::new().read(true).write(true).open(&name);
        assert_eq!(
            denied.err().and_then(|e| e.raw_os_error()),
            Some(5),
            "GENERIC_WRITE must be refused by the DACL (ERROR_ACCESS_DENIED)"
        );
        // Unblock the server's second accept.
        let _ = connect(&name, Duration::from_secs(2));
        let _ = server.join();
    }
}
