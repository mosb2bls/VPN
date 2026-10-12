# EdgeClient

Personal desktop proxy client (macOS Apple Silicon, Windows x64) for a single self-hosted
server: Hysteria 2 (primary) and VLESS + REALITY + XTLS Vision (fallback), using the
[sing-box](https://github.com/SagerNet/sing-box) core (pinned, checksum-verified).
"EdgeClient" is a working name.

- Contract between backend and UI: [`docs/SPEC.md`](docs/SPEC.md)
- Tests and results: [`docs/TESTING.md`](docs/TESTING.md) · design decisions: [`docs/adr/`](docs/adr/)
- Agent rules: [`CLAUDE.md`](CLAUDE.md)

## Modes

- **System proxy** (no admin rights): sets the macOS proxy for every network service to a
  local sing-box. Covers only apps that honour the system proxy; cannot stop IPv6 bypass;
  no kill switch.
- **TUN** (macOS; needs a one-time helper install with your administrator password): all
  apps, IPv6 captured and rejected, firewall kill switch if the core crashes.

## Install (unsigned builds)

Builds are not code-signed or notarised.

**macOS (Apple Silicon).** Open the `.dmg`, drag EdgeClient to Applications. The first
launch is blocked by Gatekeeper ("cannot be opened because Apple cannot check it…"):

1. Try to open it once, then open **System Settings → Privacy & Security**, scroll to
   "EdgeClient was blocked…", click **Open Anyway**, confirm with your password; or
2. In Terminal: `xattr -dr com.apple.quarantine /Applications/EdgeClient.app`

The app lives in the menu bar (no Dock icon). TUN mode asks for your administrator
password once to install its helper (Settings → Helper).

**Windows (x64).** Run the `…-setup.exe`. SmartScreen may show "Windows protected your
PC": click **More info → Run anyway**. (TUN / Windows service: not yet available.)

## If networking is left in a bad state

- Tray menu → **Restore networking**, or
- Terminal: `/Applications/EdgeClient.app/Contents/MacOS/edge-app --restore-networking`
  (prints a per-item report; exit code 0 = restored).

## Build from source

Requires Rust (stable), Node.js, Xcode command line tools (macOS).

```
npm ci
npm run fetch-core          # downloads + verifies sing-box and the rule-sets (not committed)
cargo test --workspace      # Rust tests (some run the real sing-box)
npm run lint                # frontend lint
cargo run -p edge-app       # run the app against a running `npm run dev` (port 1420)
npm run bundle:mac          # .dmg (macOS)   |   npm run bundle:win   # NSIS installer (Windows)
```

The real-server test harness (`npm run harness`) is local-only and reads share links from
a directory you point it to; see `docs/TESTING.md` §4. Never commit links or secrets.

## Privacy

No telemetry. Network access only to: your proxy server, DNS resolvers (AliDNS / Quad9
direct, Cloudflare via the proxy), probe targets (gstatic, Cloudflare), rule-set sources
(GitHub, SagerNet repositories). Credentials are stored in the OS keychain; logs and
diagnostics are redacted.
