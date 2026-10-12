#!/usr/bin/env node
// Before `tauri build`: copy the release helper binary next to the fetched
// sing-box as a Tauri sidecar (`binaries/edge-helper-<triple>`). macOS only
// (the Windows service arrives in the Windows phase).
import { copyFileSync, existsSync, chmodSync } from "node:fs";
import { dirname, join } from "node:path";
import { fileURLToPath } from "node:url";

const root = join(dirname(fileURLToPath(import.meta.url)), "..");
if (process.platform !== "darwin") {
  console.log("[prepare-bundle] nothing to do on this platform");
  process.exit(0);
}
const triple = process.arch === "arm64" ? "aarch64-apple-darwin" : "x86_64-apple-darwin";
const src = join(root, "target", "release", "edge-helper");
if (!existsSync(src)) {
  console.error("[prepare-bundle] build the helper first: cargo build --release -p edge-helper");
  process.exit(1);
}
const dst = join(root, "src-tauri", "binaries", `edge-helper-${triple}`);
copyFileSync(src, dst);
chmodSync(dst, 0o755);
console.log(`[prepare-bundle] ${dst}`);
