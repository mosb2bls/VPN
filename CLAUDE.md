# Lynnk — instructions for all agents

Personal desktop VPN/proxy client (Tauri v2 + Rust + sing-box) for macOS arm64 and
Windows x64, connecting only to the owner's server (Hysteria2 UDP 24443 primary,
VLESS + REALITY + XTLS Vision TCP 443 fallback). The product name is "Lynnk" (owner
decision 2026-10-12; formerly the placeholder "EdgeClient"), defined in
`crates/core/src/lib.rs` (`APP_NAME`), `src-tauri/tauri.conf.json` and, for the UI,
`src/lib/brand.ts`. Identifiers keep `edgeclient` (`io.github.mosb2bls.edgeclient`, the
helper service and paths, the pf anchor) so existing keychain items and installs survive.

Two Claude Code sessions work in this one folder on `main`, plus a third on the owner's
Windows PC:

- **Backend agent** — all functional code (Rust, Tauri commands/events, helper,
  sing-box, networking, rules, DNS, failover, tests, scripts, CI) and owner of
  `docs/SPEC.md`, the contract the UI is built against.
- **UI agent** — all frontend code (React + TypeScript + Vite).
- **Windows tester** — runs the Windows checklist (`docs/TESTING.md` §3) on the owner's
  Windows 11 PC; only tests and diagnoses, never changes code on `main`; writes redacted
  reports to `docs/windows-test/` and pushes them (commit prefix `wintest:`).

They never edit each other's files and collaborate only through the files below.

## 1. File ownership

| Path | Owner |
|---|---|
| `src/` (all frontend code, incl. `src/api/mock.ts`, `src/api/client.ts`) | UI |
| `src/api/bindings.ts` (generated TS types/commands) | Backend (generated; UI must not hand-edit) |
| `docs/UI.md` | UI |
| `package.json` frontend dependencies, `vite.config.ts`, `index.html`, `tsconfig*.json`, `public/`, frontend lint/format config | UI |
| `design/`, `components.json`, `.claude/launch.json` (dev-server command only) | UI |
| `src-tauri/`, `crates/`, `resources/`, `scripts/`, `tests/`, `.github/` | Backend |
| `package.json` `scripts` section, `tauri.conf.json`, `Cargo.*` | Backend |
| `docs/SPEC.md`, `docs/TESTING.md`, `docs/adr/` | Backend |
| `docs/windows-test/` (redacted Windows test reports) | Windows tester |
| `docs/collab/REQUESTS.md` | Shared (rules below) |
| `docs/collab/STATUS.md` | Shared (each agent edits only its own section) |
| `CLAUDE.md`, `.claude/settings.json` | Backend creates; changes need the owner's approval |
| `.gitignore`, `README.md` | Backend |

## 2. Collaboration protocol

1. **Never edit a file you don't own.** If you need a change there, write a request (rule 4).
2. **Commit only your own paths**, message prefix `backend:`, `ui:` or `wintest:`. Use
   `git add <owned paths>` — never `git add -A` or `git add .`. Never commit,
   revert, reformat or stash the other agent's uncommitted work. If `git status`
   shows the other agent's changes, leave them alone.
3. **`docs/SPEC.md` is the contract.** It has a semver version and a changelog at
   the top. Every backend change that affects the interface must, in the same
   commit: update SPEC.md, bump its version (major = breaking, minor = additive,
   patch = clarification), add a changelog entry, and regenerate
   `src/api/bindings.ts`.
4. **`docs/collab/REQUESTS.md`** is the request log between agents. Entry format:
   ```
   ## REQ-<number>: <short title>
   - From: ui | backend
   - Date:
   - Status: open | accepted | done (SPEC vX.Y.Z) | rejected (reason)
   - Need: what is needed and why (which screen/state)
   - Proposed shape: optional
   - Response: (filled in by the other agent)
   ```
   Only the requester creates an entry; only the receiver changes Status and
   writes Response. Never delete entries.
5. **`docs/collab/STATUS.md`** has `## Backend` and `## UI` sections. Each agent
   keeps its own section current: phase/step, SPEC version built against, what
   is real vs stubbed, known issues.
6. **At the start of every step:** `git pull` (once a remote exists), then read the
   latest SPEC.md changelog, open items in REQUESTS.md, and the other agent's
   STATUS.md section.
7. **Breaking changes** (SPEC major bump) require the owner's approval first and
   are announced in a "Message for UI agent" block.
8. **Handoff messages:** whenever the other agent must react (new SPEC version,
   request answered, breaking change), include a short "Message for UI agent" /
   "Message for backend agent" block in the checkpoint report. The owner relays it.
9. **The app must always build.** Stubbed commands return clearly marked
   placeholder data or the error code `NOT_IMPLEMENTED`, never panic.

### Checkpoint rule (both agents)

Work in small steps. After each step stop and report: what changed (files +
2–5 line summary); what the owner should look at/test and how; open decisions
with a recommendation; a "Message for the other agent" block (or "none").
Then wait for the owner's reply.

## 3. Priorities (in order — never trade a higher one for a lower one)

1. Reliable connections.
2. Correct DNS and IPv6 handling (no leaks).
3. Clean disconnect, crash recovery and restoration of the system's networking.
4. Everything else.

If something conflicts, tell the owner.

## 4. Honesty rules

- Never claim a feature works until it has been tested. Mark untested items as **untested**.
- Never claim system-proxy mode protects the whole device. It only covers apps
  that honour the system proxy, cannot stop IPv6 bypass, and has no kill switch.
- When sing-box, Tauri or OS behaviour is uncertain, check the official
  documentation for the pinned version instead of relying on memory, and cite the
  doc page in the relevant ADR.

## 5. Secret-handling rules (mandatory for every agent)

- **Real share links live outside the repo** in `/Users/rw/.config/personal-edge-proxy/`
  (also holds Cloudflare credentials and client configs). The env var
  `EDGE_TEST_LINKS` points to that **directory**. Agents must never read, `cat`,
  `grep`, list contents of, copy, or otherwise load anything under that directory
  into the conversation, and must never echo `$EDGE_TEST_LINKS`-derived file
  contents. This is enforced by deny rules in `.claude/settings.json`; never
  weaken or work around them. If a deny rule blocks running the harness, give
  the owner the command to run instead.
- **Pre-existing deployment files in the repo root** (`add_reality.py`,
  `check_b_layer.py`, `configure_hy2.py`, `deployment-notes.md`,
  `save_cloudflare_token.py`, `xray-install-result.log`,
  `xray-install-reviewed.sh`) belong to the owner, may contain secrets, are
  gitignored, and must not be read, edited, moved or committed by agents.
- Only the local integration harness (`tests/integration/`) reads the link files,
  at runtime. It passes settings to sing-box via protected temporary config files
  (mode 0600, removed after the run), never via shell arguments or command-line
  interpolation, and prints only a redacted summary: node type, pass/fail,
  latency, exit-IP **country**, HTTP status, error codes. Never hostnames, IPs,
  passwords, UUIDs, keys, short IDs or links.
- Never commit secrets, link files, downloaded binaries, generated runtime
  configs, journals or diagnostics. Never put real credentials in CI, build
  artifacts, screenshots, test fixtures or diagnostic uploads. Tests use fake
  values only.
- Secrets are redacted in logs, forwarded sing-box output, diagnostics, errors and
  panics; the UI only ever receives masked values (e.g. `a1b2…••••`).
- No telemetry. External calls only to: the proxy server, DNS resolvers, probe
  targets, rule-set sources (and sing-box/rule downloads at build time).

## 6. Do not

- Modify the server or require Xray changes — the client adapts to the server.
- Add features beyond the agreed scope without asking (subscriptions, screen QR
  scanning, stats history, other protocols).
- Mark anything as working or compatible without a real test.

## 7. Layout

```
CLAUDE.md  .claude/settings.json
docs/        SPEC.md TESTING.md UI.md(ui) adr/ collab/{REQUESTS,STATUS}.md
crates/core    pure Rust: parsing, config gen, rules, DNS, failover logic, redaction
crates/helper  privileged helper/service (TUN, kill switch)
src-tauri/     Tauri app: commands, events, state, tray, helper client
src/           UI agent only (except generated src/api/bindings.ts)
resources/rules/  bundled .srs rule-sets + manifest
scripts/       download/verify sing-box and rules
tests/integration/  local-only real-server harness
.github/workflows/
```

Useful commands: `npm run tauri dev` (run app), `cargo test --workspace` (Rust tests).
