import React from "react";
import ReactDOM from "react-dom/client";
import { windowLabel } from "./api/client";
import { DevPage } from "./dev/DevPage";
import { Popover } from "./popover/Popover";
import { Settings } from "./settings/Settings";
import { startStore } from "./state/store";
import { ErrorBoundary } from "./ui/ErrorBoundary";
import "./design/tokens.css";
import "./dev/dev.css";

// Route on the Tauri window label (SPEC §2.8: `popover` / `settings`). In a
// plain browser, show the dev page with both side by side.
// Dev: `#popover` / `#settings` in a plain browser render one window on its own, exactly as
// the app does (size the viewport to 340×480 or 760×560 to mirror the real window).
const devSingle = !windowLabel() && (location.hash === "#popover" || location.hash === "#settings") ? location.hash.slice(1) : null;
const label = windowLabel() ?? devSingle;
// App windows never scroll at page level (Windows F-2); see tokens.css.
if (label) document.documentElement.dataset.window = label;
const root =
  label === "popover" ? <Popover /> : label === "settings" ? <Settings /> : <DevPage />;

void startStore();

ReactDOM.createRoot(document.getElementById("root") as HTMLElement).render(
  <React.StrictMode>
    <ErrorBoundary>{root}</ErrorBoundary>
  </React.StrictMode>,
);
