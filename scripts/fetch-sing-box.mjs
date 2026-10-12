#!/usr/bin/env node
// Download, verify (SHA-256) and install the pinned sing-box core into
// src-tauri/binaries/ as `sing-box-<target-triple>[.exe]`, plus a `.sha256`
// file with the extracted binary's hash (embedded into the app at build time
// and checked before every launch).
//
// Usage:  node scripts/fetch-sing-box.mjs            # host target only
//         node scripts/fetch-sing-box.mjs --all      # every target in the lock file
//         node scripts/fetch-sing-box.mjs --target x86_64-pc-windows-msvc
//
// Safe to re-run: an installed binary whose hash matches is left alone.
// Downloads only from github.com/SagerNet/sing-box releases. Never commits anything.

import { createHash } from "node:crypto";
import { execFileSync } from "node:child_process";
import { chmodSync, copyFileSync, existsSync, mkdirSync, mkdtempSync, readFileSync, readdirSync, rmSync, statSync, writeFileSync } from "node:fs";
import { tmpdir } from "node:os";
import { dirname, join } from "node:path";
import { fileURLToPath } from "node:url";

const root = join(dirname(fileURLToPath(import.meta.url)), "..");
const lock = JSON.parse(readFileSync(join(root, "scripts", "sing-box.lock.json"), "utf8"));
const outDir = join(root, "src-tauri", "binaries");

function hostTriple() {
  const { platform, arch } = process;
  if (platform === "darwin" && arch === "arm64") return "aarch64-apple-darwin";
  if (platform === "win32" && arch === "x64") return "x86_64-pc-windows-msvc";
  return null;
}

function sha256(buf) {
  return createHash("sha256").update(buf).digest("hex");
}

function findFile(dir, name) {
  for (const entry of readdirSync(dir)) {
    const p = join(dir, entry);
    if (statSync(p).isDirectory()) {
      const r = findFile(p, name);
      if (r) return r;
    } else if (entry === name) {
      return p;
    }
  }
  return null;
}

async function install(triple) {
  const t = lock.targets[triple];
  if (!t) throw new Error(`no entry for target ${triple} in sing-box.lock.json`);
  const exe = triple.includes("windows") ? ".exe" : "";
  const dest = join(outDir, `sing-box-${triple}${exe}`);
  const hashFile = join(outDir, `sing-box-${triple}.sha256`);

  if (existsSync(dest) && existsSync(hashFile) && sha256(readFileSync(dest)) === readFileSync(hashFile, "utf8").trim()) {
    console.log(`[fetch-core] ${triple}: sing-box ${lock.version} already installed and verified`);
    return;
  }

  const url = `https://github.com/SagerNet/sing-box/releases/download/v${lock.version}/${t.asset}`;
  console.log(`[fetch-core] ${triple}: downloading ${t.asset}`);
  const res = await fetch(url, { redirect: "follow" });
  if (!res.ok) throw new Error(`download failed: HTTP ${res.status}`);
  const archive = Buffer.from(await res.arrayBuffer());
  const got = sha256(archive);
  if (got !== t.sha256) {
    throw new Error(`SHA-256 MISMATCH for ${t.asset}\n  expected ${t.sha256}\n  got      ${got}\nRefusing to install.`);
  }
  console.log(`[fetch-core] ${triple}: archive SHA-256 verified (${archive.length} bytes)`);

  const work = mkdtempSync(join(tmpdir(), "edge-singbox-"));
  try {
    const archivePath = join(work, t.asset);
    writeFileSync(archivePath, archive);
    const extractDir = join(work, "x");
    mkdirSync(extractDir);
    // bsdtar (macOS, Windows 10+) extracts both .tar.gz and .zip. Arguments are
    // passed as an array — no shell.
    // On Windows use the system bsdtar explicitly: Git's GNU tar (often first on
    // PATH on CI runners) cannot extract .zip archives.
    const tar = process.platform === "win32"
      ? join(process.env.SystemRoot || "C:\\Windows", "System32", "tar.exe")
      : "tar";
    execFileSync(tar, ["-xf", archivePath, "-C", extractDir], { stdio: "inherit" });
    const bin = findFile(extractDir, t.binary);
    if (!bin) throw new Error(`${t.binary} not found in ${t.asset}`);
    mkdirSync(outDir, { recursive: true });
    copyFileSync(bin, dest);
    if (!exe) chmodSync(dest, 0o755);
    const binHash = sha256(readFileSync(dest));
    writeFileSync(hashFile, binHash + "\n");
    console.log(`[fetch-core] ${triple}: installed ${dest}\n[fetch-core] ${triple}: binary SHA-256 ${binHash}`);
  } finally {
    rmSync(work, { recursive: true, force: true });
  }

  if (triple === hostTriple()) {
    const out = execFileSync(dest, ["version"], { encoding: "utf8" });
    const first = out.split("\n")[0];
    if (!first.includes(lock.version)) throw new Error(`unexpected version output: ${first}`);
    console.log(`[fetch-core] ${triple}: ${first}`);
  }
}

const args = process.argv.slice(2);
let targets;
if (args.includes("--all")) targets = Object.keys(lock.targets);
else if (args.includes("--target")) targets = [args[args.indexOf("--target") + 1]];
else {
  const h = hostTriple();
  if (!h) {
    console.error(`[fetch-core] unsupported host ${process.platform}/${process.arch}; use --target`);
    process.exit(1);
  }
  targets = [h];
}

try {
  for (const t of targets) await install(t);
} catch (e) {
  console.error(`[fetch-core] ERROR: ${e.message}`);
  process.exit(1);
}
