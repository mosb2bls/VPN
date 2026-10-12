//! EdgeClient privileged helper: macOS launchd daemon (ADR-0005) or Windows
//! service (ADR-0007).
//!
//!   edge-helper serve                                           (launchd / SCM)
//!   edge-helper install --uid <uid> --sing-box <path> --rules <dir>   (macOS, root)
//!   edge-helper install --sid <SID> --sing-box <path> --rules <dir>   (Windows, elevated)
//!         [--report <file>]   write the error text there (Windows: no console)
//!   edge-helper uninstall
//!   edge-helper version
//!
//! Never executes arbitrary commands or paths: it only runs its own installed,
//! hash-verified sing-box with configs it generates itself.

#[cfg(any(target_os = "macos", windows))]
mod rpc;
#[cfg(any(target_os = "macos", windows))]
mod session;
#[cfg(any(target_os = "macos", windows))]
mod sys;
#[cfg(any(target_os = "macos", windows))]
mod util;

#[cfg(target_os = "macos")]
mod install;
#[cfg(target_os = "macos")]
mod server;

#[cfg(windows)]
mod install_win;
#[cfg(windows)]
mod server_win;

fn main() {
    let args: Vec<String> = std::env::args().collect();
    let code = run(&args);
    std::process::exit(code);
}

#[cfg(any(target_os = "macos", windows))]
fn run(args: &[String]) -> i32 {
    let flag = |n: &str| args.iter().position(|a| a == n).and_then(|i| args.get(i + 1)).cloned();
    let result = match args.get(1).map(String::as_str) {
        #[cfg(target_os = "macos")]
        Some("serve") => server::serve(),
        #[cfg(windows)]
        Some("serve") => server_win::serve(),
        #[cfg(target_os = "macos")]
        Some("install") => {
            let uid = flag("--uid").and_then(|u| u.parse().ok());
            match (uid, flag("--sing-box"), flag("--rules")) {
                (Some(uid), Some(sb), Some(rules)) => install::install(uid, sb.as_ref(), rules.as_ref()),
                _ => Err("usage: install --uid <uid> --sing-box <path> --rules <dir>".into()),
            }
        }
        #[cfg(windows)]
        Some("install") => match (flag("--sid"), flag("--sing-box"), flag("--rules")) {
            (Some(sid), Some(sb), Some(rules)) => install_win::install(&sid, sb.as_ref(), rules.as_ref()),
            _ => Err("usage: install --sid <SID> --sing-box <path> --rules <dir>".into()),
        },
        #[cfg(target_os = "macos")]
        Some("uninstall") => install::uninstall(),
        #[cfg(windows)]
        Some("uninstall") => install_win::uninstall(),
        Some("version") => {
            println!("{} protocol {}", env!("CARGO_PKG_VERSION"), edge_core::helper_proto::PROTOCOL_VERSION);
            Ok(())
        }
        _ => Err("usage: edge-helper serve | install | uninstall | version".into()),
    };
    match result {
        Ok(()) => 0,
        Err(e) => {
            eprintln!("{} helper: {e}", edge_core::APP_NAME);
            // Elevated runs on Windows have no console the app can read.
            if let Some(report) = flag("--report") {
                let _ = std::fs::write(report, edge_core::redact::Redactor::new().redact(&e));
            }
            1
        }
    }
}

#[cfg(not(any(target_os = "macos", windows)))]
fn run(_: &[String]) -> i32 {
    eprintln!("{} helper: macOS and Windows only", edge_core::APP_NAME);
    2
}
