//! Embed the expected sing-box binary hash (from `npm run fetch-core`) and the
//! rule-set manifest, so the helper only ever runs / uses verified files.
fn main() {
    let target = std::env::var("TARGET").expect("TARGET");
    let hash_path = format!("../../src-tauri/binaries/sing-box-{target}.sha256");
    println!("cargo:rerun-if-changed={hash_path}");
    let hash = std::fs::read_to_string(&hash_path).map(|s| s.trim().to_string()).unwrap_or_default();
    println!("cargo:rustc-env=HELPER_SINGBOX_SHA256={hash}");
    println!("cargo:rerun-if-changed=../../resources/rules/manifest.json");
}
