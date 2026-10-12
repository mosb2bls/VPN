#!/usr/bin/env node
// Before `tauri build`: copy the release helper binary next to the fetched
// sing-box as a Tauri sidecar (`binaries/edge-helper-<triple>[.exe]`).
// macOS: launchd daemon (ADR-0005). Windows: service (ADR-0007).
import { copyFileSync, existsSync, chmodSync } from "node:fs";
import { dirname, join } from "node:path";
import { fileURLToPath } from "node:url";

const root = join(dirname(fileURLToPath(import.meta.url)), "..");
const targets = {
  "darwin-arm64": { triple: "aarch64-apple-darwin", ext: "" },
  "win32-x64": { triple: "x86_64-pc-windows-msvc", ext: ".exe" },
};
const t = targets[`${process.platform}-${process.arch}`];
if (!t) {
  console.error(`[prepare-bundle] unsupported platform ${process.platform}-${process.arch}`);
  process.exit(1);
}
const src = join(root, "target", "release", `edge-helper${t.ext}`);
if (!existsSync(src)) {
  console.error("[prepare-bundle] build the helper first: cargo build --release -p edge-helper");
  process.exit(1);
}
const dst = join(root, "src-tauri", "binaries", `edge-helper-${t.triple}${t.ext}`);
copyFileSync(src, dst);
if (t.ext === "") chmodSync(dst, 0o755);
console.log(`[prepare-bundle] ${dst}`);
