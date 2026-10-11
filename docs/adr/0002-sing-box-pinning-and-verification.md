# ADR-0002: Pinning, fetching and verifying the sing-box core

- Status: accepted (Phase B2.4, 2026-10-12)

## Context

The brief requires sing-box as the only core, an exact pinned version (≥ 1.13.x, latest
stable at build time), downloaded at build time from official GitHub releases, SHA-256
verified, never committed; and the app (and later the privileged helper) must only run the
bundled, checksum-verified binary.

## Decision

- **Version:** sing-box **1.14.3**, the latest stable release on 2026-10-12
  (1.15.0 is alpha). Recorded in **one place**: `scripts/sing-box.lock.json`.
- **Checksums:** the lock file pins the SHA-256 of each release archive, taken from the
  GitHub release API's per-asset `digest` field
  (`GET https://api.github.com/repos/SagerNet/sing-box/releases/tags/v1.14.3`), i.e.
  computed by GitHub at upload. Targets: `aarch64-apple-darwin`
  (`sing-box-1.14.3-darwin-arm64.tar.gz`), `x86_64-pc-windows-msvc`
  (`sing-box-1.14.3-windows-amd64.zip`).
- **Fetch:** `npm run fetch-core` (`scripts/fetch-sing-box.mjs`, Node, no extra deps)
  downloads from `github.com/SagerNet/sing-box/releases/download/v<ver>/<asset>`, refuses
  to install on any archive hash mismatch, extracts with the system `tar` (bsdtar handles
  `.tar.gz` and `.zip` on macOS and Windows 10+; arguments passed without a shell), and
  installs `src-tauri/binaries/sing-box-<triple>[.exe]` (gitignored) plus a
  `.sha256` file with the **extracted binary's** hash. On the host target it also checks
  `sing-box version` reports the pinned version. Re-runs are no-ops when already verified.
- **Embedding:** `src-tauri/build.rs` embeds the pinned version
  (`EDGE_SINGBOX_VERSION`) and the expected binary hash (`EDGE_SINGBOX_SHA256`) for the
  build target. A build without the fetched core still succeeds (the hash is empty) —
  "the app must always build" — and the app reports `CORE_BINARY_MISSING` on connect.
- **Runtime:** `core_bin::locate_verified()` looks next to the app executable (bundled
  builds) and, in debug builds only, in `src-tauri/binaries/`; it hashes the file and refuses
  to run it unless it matches (`CORE_CHECKSUM_MISMATCH`, non-recoverable). There is no
  user-configurable core path.
- **Bundling:** Tauri `bundle.externalBin` is **not** enabled yet: it makes `cargo build`
  fail when the binary is absent. It is enabled in Phase B6 (packaging/CI), where CI runs
  `npm run fetch-core -- --all` first.

## Consequences

- Upgrading sing-box = edit the lock file (version + both hashes from the API), run
  `npm run fetch-core`, rebuild, re-run the integration harness.
- The user-level app verifies before each launch, but the binary lives in a user-writable
  location, so this protects against corruption/accidental replacement, not against a local
  attacker running as the same user. The privileged helper (B3) will copy the binary into a
  root/SYSTEM-owned location and verify it there before running it with privileges.
- 1.14 lacks full-certificate pinning (`certificate_sha256`, 1.15+), so HY2 `pinSHA256`
  links are rejected (SPEC §2.1.1) until we move to 1.15 stable.

## Verified (2026-10-12, macOS arm64)

- `npm run fetch-core`: archive hash verified, binary installed, `sing-box version 1.14.3`.
- Re-run: "already installed and verified".
- Tamper test: wrong expected hash for the Windows asset → "SHA-256 MISMATCH … Refusing to
  install", nothing installed.
- `npm run fetch-core -- --all`: Windows asset verified and extracted on macOS.
- `cargo build` with `src-tauri/binaries/` removed: succeeds.
