//! EdgeClient privileged helper (macOS launchd daemon). See docs/adr/0005.
//!
//!   edge-helper serve                                       (launchd)
//!   edge-helper install --uid <uid> --sing-box <path> --rules <dir>   (root, once)
//!   edge-helper uninstall                                   (root)
//!   edge-helper version
//!
//! Never executes arbitrary commands or paths: it only runs its own installed,
//! hash-verified sing-box with configs it generates itself.

#[cfg(target_os = "macos")]
mod install;
#[cfg(target_os = "macos")]
mod pfctl;
#[cfg(target_os = "macos")]
mod server;
#[cfg(target_os = "macos")]
mod session;
#[cfg(target_os = "macos")]
mod util;

fn main() {
    let args: Vec<String> = std::env::args().collect();
    let code = run(&args);
    std::process::exit(code);
}

#[cfg(target_os = "macos")]
fn run(args: &[String]) -> i32 {
    let flag = |n: &str| args.iter().position(|a| a == n).and_then(|i| args.get(i + 1)).cloned();
    let result = match args.get(1).map(String::as_str) {
        Some("serve") => server::serve(),
        Some("install") => {
            let uid = flag("--uid").and_then(|u| u.parse().ok());
            match (uid, flag("--sing-box"), flag("--rules")) {
                (Some(uid), Some(sb), Some(rules)) => install::install(uid, sb.as_ref(), rules.as_ref()),
                _ => Err("usage: install --uid <uid> --sing-box <path> --rules <dir>".into()),
            }
        }
        Some("uninstall") => install::uninstall(),
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
            1
        }
    }
}

#[cfg(not(target_os = "macos"))]
fn run(_: &[String]) -> i32 {
    eprintln!("{} helper: Windows service is implemented in the Windows phase", edge_core::APP_NAME);
    2
}
