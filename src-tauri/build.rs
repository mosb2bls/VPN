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

    // Windows: tauri-build embeds its manifest only into the app binary; unit-test
    // binaries then fail to load (STATUS_ENTRYPOINT_NOT_FOUND). Embed the same
    // manifest ourselves for every target of this package instead.
    let mut attrs = tauri_build::Attributes::new();
    if std::env::var("CARGO_CFG_TARGET_OS").as_deref() == Ok("windows")
        && std::env::var("CARGO_CFG_TARGET_ENV").as_deref() == Ok("msvc")
    {
        let manifest = std::path::Path::new(&std::env::var("CARGO_MANIFEST_DIR").expect("CARGO_MANIFEST_DIR"))
            .join("windows-app-manifest.xml");
        println!("cargo:rerun-if-changed={}", manifest.display());
        println!("cargo:rustc-link-arg=/MANIFEST:EMBED");
        println!("cargo:rustc-link-arg=/MANIFESTINPUT:{}", manifest.display());
        attrs = attrs.windows_attributes(tauri_build::WindowsAttributes::new_without_app_manifest());
    }
    tauri_build::try_build(attrs).expect("tauri-build")
}
