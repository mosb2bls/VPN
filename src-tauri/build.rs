//! Embeds the pinned sing-box version and the expected binary hash for the
//! build target. The hash file is produced by `npm run fetch-core` (after the
//! release archive's SHA-256 has been verified against scripts/sing-box.lock.json).
//! Building without it is allowed — the app then reports CORE_BINARY_MISSING.

fn main() {
    let lock_path = "../scripts/sing-box.lock.json";
    println!("cargo:rerun-if-changed={lock_path}");
    let lock: serde_json::Value =
        serde_json::from_str(&std::fs::read_to_string(lock_path).expect("read sing-box.lock.json"))
            .expect("parse sing-box.lock.json");
    let version = lock["version"].as_str().expect("lock: version");
    println!("cargo:rustc-env=EDGE_SINGBOX_VERSION={version}");

    let target = std::env::var("TARGET").expect("TARGET");
    let hash_path = format!("binaries/sing-box-{target}.sha256");
    println!("cargo:rerun-if-changed={hash_path}");
    let hash = std::fs::read_to_string(&hash_path).map(|s| s.trim().to_string()).unwrap_or_default();
    println!("cargo:rustc-env=EDGE_SINGBOX_SHA256={hash}");
    println!("cargo:rustc-env=EDGE_TARGET_TRIPLE={target}");

    tauri_build::build()
}
