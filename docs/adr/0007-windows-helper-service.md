# ADR-0007: Privileged helper and kill switch for TUN mode (Windows)

- Status: **proposed** (2026-10-12) — design only, nothing implemented. Implementation is
  Phase B3.2 and starts once CI builds the Windows app green. Everything here stays
  **untested** until the owner runs the Windows checklist.

## Context

Same requirements as macOS (ADR-0005, ADR-0006): the UI process stays unprivileged; a
privileged component runs sing-box with TUN, using only the bundled, hash-verified binary
and configs it generates itself from a closed request enum; minimal authenticated local
IPC; a journal so a crash or reboot never leaves networking broken; and a kill switch that
blocks traffic only while the core is down unexpectedly.

## Proposed decisions

**Service.** `edge-helper.exe` doubles as a Windows service (`serve` subcommand), run by the
Service Control Manager as LocalSystem with automatic start, so it can recover at boot.
The SCM restarts it on failure. The same `edge-helper` crate holds both platforms. It has
no `ring` dependency, so the Windows code can be compile-checked from macOS (as
`crates/winsys` is).

**Install / uninstall.** `install_helper` launches `edge-helper.exe install` elevated
(`ShellExecuteExW`, verb `runas` → the standard UAC prompt; cancel →
`HELPER_INSTALL_CANCELLED`). `install` does the following:
- refuses unless the bundled sing-box matches the embedded SHA-256 (as on macOS);
- copies the helper, sing-box and rule-sets to `%ProgramFiles%\EdgeClient\Helper\`
  (writable by administrators only, so a user process cannot swap the binaries);
- creates the service and sets its failure actions;
- records the installing user's SID.

`uninstall` stops the service, restores networking, deletes the service and removes the
files.

**IPC.** The service creates the named pipe `\\.\pipe\io.github.mosb2bls.edgeclient.helper`
with these settings:
- an explicit security descriptor: SYSTEM full control, the installing user's SID read/write,
  nobody else. Without one, Windows grants Everyone read access (Microsoft Learn,
  `CreateNamedPipeW`, `lpSecurityAttributes`).
- `FILE_FLAG_FIRST_PIPE_INSTANCE`, so the pipe cannot be pre-created by another process
  (squatting).
- `PIPE_REJECT_REMOTE_CLIENTS`.

The client checks that the pipe server process is the installed service binary before
sending anything. The protocol is the same JSON-lines `helper_proto` as on macOS.

**TUN.** sing-box's TUN on Windows uses Wintun, which sing-tun embeds in the binary
(`internal/wintun/dll_windows_amd64.go`: `//go:embed amd64/wintun.dll`), so no separate DLL
ships. The config generator is unchanged:
- IPv4 + IPv6 TUN addresses, `auto_route`, `auto_detect_interface`;
- `strict_route`, which on Windows also blocks DNS leaks from multihomed resolution (sing-box
  docs, Tun inbound: "prevent DNS leak caused by Windows' ordinary multihomed DNS
  resolution behavior");
- IPv6 routed into the TUN and rejected, as on macOS.

`auto_redirect` is Linux-only and is not used.

**Kill switch: Windows Filtering Platform.** The kill switch is engaged only when the core
dies unexpectedly, as with pf on macOS:
- filters in our own sublayer block IPv4/IPv6 connect and accept at the ALE layers;
- they permit loopback, DHCP, LAN (if allowed) and the bundled sing-box executable (by app
  ID, so the restart can reach the server).

The filters are added in a **dynamic WFP session** (`FWPM_SESSION_FLAG_DYNAMIC`: "any
objects added during the session are automatically deleted when the session ends").
Therefore, if the service itself dies, the block disappears instead of locking the PC
offline. The SCM restart then recovers from the journal. This is the same "never strand the
user" trade-off as the macOS pf token.

**Journal and recovery.** The journal lives in `%ProgramData%\EdgeClient\Helper\`, readable
and writable by SYSTEM and administrators only. On service start, recovery works as
`recover_at_start` does on macOS: stop a stale core (only if that PID still runs our
binary) and report what was restored.

## Open questions (resolved during implementation, with tests)

- Whether WinINet system-proxy settings must also be cleared when switching to TUN.
  Disconnect already restores them, but a user may switch capture modes mid-session.
- Microsoft Defender SmartScreen and the unsigned installer: expected warnings, which the
  owner accepts for a personal build.

## References

- sing-box Tun inbound: https://sing-box.sagernet.org/configuration/inbound/tun/
- sing-box route (`auto_detect_interface`): https://sing-box.sagernet.org/configuration/route/
- sing-tun embedded Wintun: https://github.com/SagerNet/sing-tun/tree/main/internal/wintun
- `CreateNamedPipeW`: https://learn.microsoft.com/en-us/windows/win32/api/winbase/nf-winbase-createnamedpipea
- `FWPM_SESSION0`: https://learn.microsoft.com/en-us/windows/win32/api/fwpmtypes/ns-fwpmtypes-fwpm_session0
