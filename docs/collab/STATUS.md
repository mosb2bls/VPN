# Agent status

Each agent edits only its own section.

## Backend

- **Phase/step:** B0 — Setup (complete, awaiting owner checkpoint)
- **SPEC version:** none yet — `docs/SPEC.md` v0.1.0 arrives in Phase B1
- **Real:** Tauri v2 + React/TS/Vite default template scaffold; Cargo workspace
  (`src-tauri` = `edge-app`, `crates/core` = `edge-core`, `crates/helper` = `edge-helper`);
  product name constant `edge_core::APP_NAME`.
- **Stubbed:** everything functional. The only command is the template's `greet`,
  which will be removed in B1. `edge-helper` exits with code 2 ("not implemented").
- **Known issues:** none.
- **UI agent:** do not start building against the backend until the SPEC v0.1.0
  handoff message arrives. The frontend is the untouched default template.

## UI

- **Phase/step:** not started
- **SPEC version:** —
- **Real / stubbed:** —
- **Known issues:** —
