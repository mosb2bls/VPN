# EdgeClient — UI notes

Owner: UI agent. Approved design decisions, tokens and frontend structure.
Built against **SPEC v1.3.1**.

## 1. Approved decisions (U0)

| # | Decision |
|---|---|
| D1 | Reference images live in `design/reference/01…05-*.png` (1600 px copies); full-size originals in `design/reference/originals/` (gitignored). |
| D2 | Popover window is **opaque with the native OS shadow** (no transparency, no `macOSPrivateApi`). The UI draws the ink frame, ticks and grid inside it. |
| D3 | **DIRECT** state: hollow ink ring (no accent colour), label `DIRECT — NOT PROXIED`. |
| D4 | Failover between two nodes of the **same protocol**: the dot stays on its tick; only the node name and badge change. |
| D5 | Colour: ink `#161512`, paper `#F2EEE5` (printed values from the references). Accents sampled: cobalt `#2439BC`, vermilion `#D34F2E`, mustard `#E7C24E`. |
| D6 | Contrast: vermilion for dot / fills / borders only; `#B8401F` where red text is unavoidable (4.8:1). `ink-muted` ≈ 65 % ink = `#625F58` (≈ 5:1). Mustard only as a fill with ink text. |
| D7 | Popover state list: OFF, CONNECTING, CONNECTED, DEGRADED, FAILING OVER, RECONNECTING, DIRECT, DISCONNECTING, RECOVERING, ERROR, BLOCKED (`killSwitchActive`), HELPER MISSING (TUN + helper not installed). |
| D9 | Mono font: **IBM Plex Mono** (JetBrains Mono removed). |
| D10 | Grid module **24 px**: popover frame 288 × 408 = 12 × 17 modules, 26 px side margins, 36 px top/bottom label bands. |
| D11 | State word width **82 %** (Archivo 850, 30 px). |
| D12 | DIRECT: the big word is **DIRECT**; directly under it a mono line `NOT PROXIED — NO PROTECTION`. Hollow ink ring dot (D3). |
| D13 | Mustard dot always has a 1 px ink outline (mustard vs paper is 1.5:1) and is always paired with the state word. |
| D14 | Red banners (BLOCKED, NOT RESTORED): vermilion border/bar with **ink text on paper**; never text on a vermilion fill. |
| D15 | Popover: **no visible grid** and almost no divider rules (owner: grid looked heavy). The 24 px rhythm, thin frame, ticks, corner labels and dot remain. One hairline above the footer. |
| D16 | Instrument band stays empty apart from the HY2 / REALITY ticks (columns 7 and 10) and the dot. |
| D17 | Restore networking is always in the popover footer (small, muted). |
| D18 | Primary button: solid ink to start or fix (CONNECT, TRY AGAIN, SET UP TUN, RESTORE NETWORKING); outline while active or busy (CANCEL, DISCONNECT, TURN OFF, DISCONNECTING…, RECOVERING…). BLOCKED uses solid DISCONNECT. |
| D19 | Notice block (rows 12–15, replaces coverage note + stats): 3 px bar — vermilion = error, mustard = warning, ink = info — with ink text on paper. Used for BLOCKED, NOT RESTORED, ERROR, HELPER MISSING, RECOVERING, and DEGRADED / RECONNECTING in system-proxy mode ("Apps may connect directly — no kill switch"). |
| D20 | Kill switch engaged **and** restore incomplete → word BLOCKED, sub "Restore incomplete — kill switch still on", failed items listed, primary RESTORE NETWORKING. |
| D21 | **State carried by the colour of the word** (owner, U2.3) — the hanging status dot and the HY2/REALITY tick labels are removed (supersedes D3, D4, D13, D16). Colours: ink (OFF, HELPER MISSING, DISCONNECTING), amber `mustard-text #94690A` (CONNECTING, DEGRADED, FAILING OVER, RECONNECTING, RECOVERING), cobalt (CONNECTED), vermilion `#D34F2E` (ERROR, BLOCKED, NOT RESTORED — large text ≥ 3:1), DIRECT = hollow outlined ink letters. The frame keeps its plain ticks. |
| D22 | Last-event line: muted when older than 10 min or from before the current connection; an action's own result (e.g. import summary, command error) takes the line until the next action. |
| D23 | Empty node list: word OFF, sub "No nodes yet", info notice with Import from clipboard / Open settings, primary ADD NODE. |
| D24 | `secretMissing` node: info-warn notice "Credentials missing — edit to re-enter", CONNECT disabled, "No creds" next to the node; Test skips it. |
| D25 | Node switcher sheet: paper-raised card (`shadow-sheet`) over a 70 % paper wash; rows 48 px — name, mono sub-line `PROTOCOL · server:port` (or "carrying traffic"), latency / Testing… / Failed / Untested / No creds on the right; cobalt bar marks the selected node. Footer: TEST ALL, MANAGE NODES. ↑/↓, Enter, Esc. |
| D26 | Motion without the dot (supersedes the dot parts of prompt §6.5 / §7.2 moments 2–3): connect/disconnect = state word slides up and fades into its new colour, primary button fill changes smoothly; failover = word turns amber, node name + protocol badge slide to the new node, reason line fades in. The frame-bow idea is dropped. |
| D27 | DIRECT stays as hollow outlined letters. |
| D8 | **NOT RESTORED** (partial restore, SPEC §2.7): shown whenever `lastRestore.fullyRestored` is false and the app is not proxying; never looks like OFF. Lists each failed/skipped item, offers Retry (Restore networking) and explains `--restore-networking`. Only an engaged kill switch (BLOCKED) ranks above it. |

## 2. Frontend structure

```
src/
  api/
    bindings.ts      generated by backend — never edit
    types.ts         Result, EventMap, Backend interface (derived from bindings)
    client.ts        the only backend entry point for components
    mock.ts          mock entry (mockBackend, mockEngine for the dev page)
    mock/            engine.ts (state machine, failover, timers, dev controls),
                     fixtures.ts, links.ts, validate.ts, errors.ts
  state/store.ts     startup snapshot + event subscriptions + activity history
  lib/format.ts      rates, durations, protocol labels
  dev/               dev page, state switcher, U1.1 debug views (temporary)
  main.tsx           routes on the Tauri window label
```

### Backend access

- Components import `commands`, `on` and `pendingWindow` from `src/api/client.ts` only.
- Every command returns `{status:"ok",data} | {status:"error",error}`. A thrown bridge
  error is converted by the client into `INTERNAL` with `detail: "IPC: …"`.
- Startup (`state/store.ts`): subscribe to all events first, then `getStatus()`,
  `listNodes()`, `getHelperStatus()`.

### Mock vs real

| Where | Default | Override |
|---|---|---|
| Plain browser (`npm run dev` → http://localhost:1420) | mock | `VITE_USE_MOCK=0` |
| Inside Tauri (`npm run tauri dev`) | real | `VITE_USE_MOCK=1 npm run tauri dev` |

The mock implements every SPEC command and event. Dev page controls:

- **Scenario**: snap into any popover state (freezes probes so the state holds).
- **Flows**: real transitions — HY2 UDP blocked → failover, failback, core crash,
  network change, wake, launch recovery (3 outcomes), scheduled rules update.
- **Outcomes**: result of connect, reconnect, restore, helper install, rules update,
  QR import, clipboard contents; per-node health (ok / flaky / fail).
- **Errors**: arm a one-shot error code on any command; raise `errorRaised` with any code.
- **Next restore (per item)**: make any of the six restore items fail or be skipped
  on the next disconnect / Restore networking / launch recovery. Scenarios
  `NOT RESTORED · TUN` / `· proxy` snap straight into a partial restore.
- **Stub mode**: behave like the current backend (B1) — stubbed commands return
  `NOT_IMPLEMENTED`.
- **Event monitor**: every event with its payload (traffic stats omitted).

Mock timings are shortened (probe round every 3 s instead of `probeIntervalSecs`);
`speed ×` scales all of them.

### Pending (not in SPEC yet)

None. (REQ-1 landed in SPEC v1.0.0; window commands/events are real.)

### Windows (SPEC §2.8)

`popover` and `settings` both load `/`; `main.tsx` routes on the window label. Esc in
the popover calls `hidePopover()`. `popoverShown` (popover only) refreshes `getStatus()`
and replays the open animation; `settingsSectionRequested` (settings only) switches the
section. The real app runs with `cargo run -p edge-app` while the Vite server is up
(`npm run tauri dev` would collide on port 1420).

## 3. Design tokens (approved U1.2)

Source of truth: `src/design/tokens.css` (Tailwind v4 `@theme`) and
`src/design/motion.ts`. Shown on the dev page → **Tokens** tab.

### Colour

| Token | Value | Use | Contrast (vs paper) |
|---|---|---|---|
| `paper` | `#F2EEE5` | window background | — |
| `paper-raised` | `#F7F4EE` | sheets, cards, inputs | — |
| `ink` | `#161512` | text, frame, primary controls | 15.8:1 |
| `ink-muted` | `#625F58` | secondary text, metadata | 5.5:1 |
| `grid` | ink 9 % | grid hairlines | decorative |
| `line` | ink 20 % | dividers, input borders | decorative |
| `cobalt` | `#2439BC` | connected, selection, focus | 7.6:1 |
| `vermilion` | `#D34F2E` | error/blocked: dot, fill, border only | 3.6:1 (non-text OK) |
| `vermilion-text` | `#B8401F` | red text when unavoidable | 4.8:1 |
| `mustard` | `#E7C24E` | connecting/degraded: fill + ink text only | 1.5:1 → needs ink outline as a dot |

No text on a vermilion fill (neither ink 4.3:1 nor paper 3.6:1 reaches 4.5:1).

### Type

- **Archivo Variable** (weights 100–900, width 62–125 %): state word, numbers, body.
- **Mono**: IBM Plex Mono 400/500/600 (D9).
- Scale (popover): `label` 10 px mono uppercase +0.14em · `label-lg` 11 · `secondary`
  12 · `body` 13 · `number` 20 tabular · `state` 30, weight 850, width 82 %.
- Utilities: `label`, `state-word`, `tabular`, `paper-grain`.

### Space, shape, elevation

- Spacing base 4 px (`--spacing: 4px`; `p-4` = 16 px).
- Corners 0 or 2 px (`rounded-xs`); borders 1 px; frame 1.5 px.
- `shadow-sheet`: `0 1px 1px ink/6 %, 0 10px 28px -10px ink/22 %` — paper card only.
- Paper grain: static SVG fractal noise, 9 % opacity, 180 px tile.

### Frame (`src/ui/Frame.tsx`)

Ink frame 1.5 px; 5 px ticks above the top edge at every grid column; 12 px registration
dashes 6 px outside each corner; corner labels 12 px outside the frame. Popover
geometry: frame 288 × 408 px, module 24 (12 × 17), 26 px side margins (D10).

### Motion

| Token | Value |
|---|---|
| `fast` / `base` / `slow` / `draw` | 120 / 200 / 320 / 480 ms |
| enter / move | `cubic-bezier(0.22, 1, 0.36, 1)` |
| exit | `cubic-bezier(0.4, 0, 1, 1)` |
| loop | `cubic-bezier(0.65, 0, 0.35, 1)` |
| stagger | 35 ms per item, max 6 |
| spring | bounce 0 only |

### Popover layout (approved U2.1, cleaned per D15)

340 × 480 window; frame 288 × 408 = 12 × 17 rows of 24 px; content inset 12 px.
Corner labels: `EDGECLIENT` · `01 / STATUS` · `<HY2|REALITY> · <UDP|TCP> <port>` · session
time (muted `00:00:00` when not connected).

| Rows | Block |
|---|---|
| 1–4 | State word (30 px, 850, 82 %, coloured per D21) + one line: progress text / consequence |
| 5–6 | Node: name, last latency, protocol badge, chevron (opens the switcher sheet) |
| 7–8 | Primary action, 40 px button (D18) |
| 9 | Routing: RULE / GLOBAL / DIRECT |
| 10 | Capture: SYSTEM PROXY / TUN |
| 11–12 | Coverage note (system proxy: required copy; TUN: whole device + kill switch) — or notice (rows 11–15, D19) |
| 13–15 | Up / Down / Latency (18 px tabular; `—` when idle) |
| 16 | Last event (failover reason or error; click for details) |
| 17 | Footer: SETTINGS · RESTORE NETWORKING · QUIT |

State → view mapping lives in `src/popover/view.ts` (priority BLOCKED > NOT RESTORED >
state; HELPER MISSING replaces OFF in TUN without helper).

### Popover states (U2.2)

| State | Word (colour) | — | Sub-line | Primary | Notice |
|---|---|---|---|---|---|
| OFF | OFF | ink, retracted | Not protected · traffic goes out normally | CONNECT (disabled without nodes) | — |
| HELPER MISSING | HELPER MISSING | ink, retracted | TUN mode needs a one-time helper install | SET UP TUN → Settings › Helper | info |
| CONNECTING | CONNECTING | mustard + ink outline, dropped | backend progress text | CANCEL (outline) | — |
| CONNECTED | CONNECTED (cobalt) | cobalt, dropped | Health checks passing / On fallback · reason | DISCONNECT (outline) | — |
| DEGRADED | DEGRADED | mustard, dropped | progress / failure count | DISCONNECT | warn in system proxy |
| FAILING OVER | FAILING OVER | mustard, dropped | Switching to … | DISCONNECT | — |
| RECONNECTING | RECONNECTING | mustard, dropped | attempt text | DISCONNECT | warn in system proxy |
| DIRECT | DIRECT | hollow ring, retracted | mono `NOT PROXIED — NO PROTECTION` | TURN OFF | — |
| DISCONNECTING | DISCONNECTING | ink, retracted | progress | DISCONNECTING… (disabled) | — |
| RECOVERING | RECOVERING | mustard, dropped | progress | RECOVERING… (disabled) | info |
| ERROR | ERROR | vermilion, retracted | reason | TRY AGAIN | error + Details |
| BLOCKED | BLOCKED | vermilion, dropped | reason | DISCONNECT | error + Restore networking |
| NOT RESTORED | NOT RESTORED | vermilion, retracted | Network settings are not back to normal | RESTORE NETWORKING | error: failed items + Details |

## 5. Motion moments

### 1. Popover open (U2.4.1)

Tokens: `openTimeline` in `src/design/motion.ts`. Plays on mount and on every
`popoverShown` (the whole popover is re-keyed). Measured in the browser: complete at
≈ 380 ms, content offset eases 6 → 0 px with no overshoot.

| Element | Start | Duration | Easing |
|---|---|---|---|
| Paper (whole popover) opacity 0 → 1 | 0 | 120 ms | enter |
| Frame outline draws (pathLength 0 → 1, from top-left, clockwise) | 20 ms | 340 ms | enter |
| Ticks fade in, left → right | 100 ms + 14 ms × i | 120 ms | enter |
| Registration dashes | after the last tick | 120 ms | enter |
| Corner labels | 160 ms | 200 ms | enter |
| Content: opacity 0 → 1, y 6 → 0 px | 100 ms | 280 ms | enter |
| Hide (Esc): opacity → 0, then `hidePopover()` | 0 | 120 ms | exit |

Parking: on `popoverHidden` (SPEC 1.2.0, sent after every backend hide) — with `blur` and
`visibilitychange → hidden` as fallbacks — the content is set to opacity 0 instantly, so the
next show starts blank instead of flashing the previous frame.

Robustness rules (found while testing in a background tab, where animation callbacks pause):
- Esc fades for 120 ms and calls `hidePopover()` on a **plain timer** of the same length —
  never on an animation-complete callback.
- If the page reports `visibilityState = hidden` when `popoverShown` arrives, the popover is
  shown **instantly** (no open animation), so it can never come up invisible.

Frame draw direction: one stroke, clockwise from the top-left (approved).

### 2. Connect / disconnect (U2.4.2)

Tokens: `stateTimeline`. Measured OFF → CONNECTING in the browser: complete ≈ 320 ms, no
overshoot.

| Element | Motion |
|---|---|
| State word | One-line counter clipped to its 32 px line: old word slides up 14 px and fades (160 ms, exit); new word rises 14 px → 0 and fades in, already in its new colour (320 ms, enter) |
| Sub-line (progress text) | Cross-fade on every change (120 ms) |
| Primary button | Fill / text / border colours transition (200 ms, enter); label cross-fades with a 3 px rise (120 ms); press = 1 px down (120 ms) |

The sub-line cross-fade is kept even though progress text changes 4–5 times while
connecting (approved).

### 3. Failover (U2.4.3)

Tokens: `swapTimeline`; component `src/ui/Swap.tsx` (old and new content share one grid
cell, so nothing jumps). Measured with Force failover in the browser: each swap completes
in ≈ 300 ms, no overshoot.

| Element | Motion |
|---|---|
| State word | CONNECTED → amber FAILING OVER → CONNECTED, via moment 2's counter |
| Node row (name, latency, badge) | Slides sideways: old out 16 px left + fade (160 ms, exit), new in from 16 px right (320 ms, enter), clipped to the row |
| "Node" / "Node · on fallback" label | Cross-fade (200 ms) |
| Bottom-left corner (`HY2 · UDP 24443` → `REALITY · TCP 443`) | Cross-fade (200 ms) |
| Last-event line (failover reason) | Cross-fade (200 ms) |
| Sub-line (`On fallback · reason`) | Moment 2 cross-fade |

Last-event line keeps its 200 ms cross-fade even when events arrive every few seconds
(approved).

### 4. Numbers (U2.4.4)

Tokens: `rollTimeline`; component `src/ui/Rolling.tsx` (own implementation — no Rare UI
code copied). Used for Up, Down, Latency and the session timer (bottom-right corner).

- Characters are matched from the right, so units (`KB/s`, `MB/s`, `ms`) keep their place.
- A changed digit rolls 70 % of the line height, clipped: up when the value grew, down
  when it shrank (180 ms, enter). Other changed characters cross-fade.
- Tabular figures + fixed stat columns: measured 0 frames of layout shift over 3 s of
  live updates.

**Motion gate** (`src/ui/motionGate.ts`): rolls, swaps, the state word and the sub-line only
animate while the popover is shown and `document.visibilityState` is `visible`. Otherwise
they render plain text. Reason: animations pause in hidden webviews, and exiting elements
would pile up (measured: up to 10 stacked glyphs per digit slot within seconds, i.e.
unbounded growth while the popover is hidden and connected). With the gate the DOM node
count stays flat.

Approved refinement: **Up / Down cross-fade** (120 ms) instead of rolling, because they change
every second; Latency and the session timer roll.

### 5. Switches (U2.4.5)

`segmentTimeline`. The ink block moves by transform (`x` in % of one option) under the
selected option, 200 ms enter; label colours cross over in the same 200 ms. The block follows
the backend's confirmed mode (no optimistic move), so it moves when the mode really changes.
Measured: one-option slide settles ≈ 190 ms after the backend confirms, no overshoot.

### 6. Node switcher sheet (U2.4.6)

`sheetTimeline`, driven by variants so open / close propagate to every part.

| Element | Motion |
|---|---|
| Wash (scrim) | opacity in 200 ms / out 120 ms |
| Card | rises 40 px + fades in, 320 ms enter (measured: settled ≈ 280 ms) / sinks + fades out, 200 ms exit |
| Rows | stagger in from 80 ms, 35 ms apart (max 6), 6 px rise (last row in ≈ 390 ms) |
| Selection bar | slides to the chosen row as soon as it is clicked (shared layout, 200 ms; measured ≈ 140 ms to arrive), then the sheet closes 180 ms after `select_node` succeeds; on error it slides back |

With the motion gate off (hidden page) the sheet opens and closes instantly.

## 6. Dependencies

| Package | Version | Why |
|---|---|---|
| react, react-dom | ^19.1 | UI framework (scaffold) |
| @tauri-apps/api | ^2 | IPC + window label (used only inside `src/api/`) |
| motion | 13.4.4 | component animation (§7 of the UI prompt) |
| tailwindcss, @tailwindcss/vite | 4.3.3 (dev) | tokens as theme variables + utilities |
| @fontsource-variable/archivo | 5.3.0 | Archivo with width axis, bundled locally (OFL) |
| @fontsource/ibm-plex-mono | 5.3.0 | mono font, bundled locally (OFL) |

Versions are pinned exactly and were published at least two weeks before install.
Font licences ship in `public/licenses/` (copied into the app bundle).
shadcn `components.json` is added with the first Radix/shadcn primitive (U2.3), not before.
