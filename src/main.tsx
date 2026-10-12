import React from "react";
import ReactDOM from "react-dom/client";
import { windowLabel } from "./api/client";
import { DevPage } from "./dev/DevPage";
import { Popover } from "./popover/Popover";
import { Settings } from "./settings/Settings";
import { startStore } from "./state/store";
import "./design/tokens.css";
import "./dev/dev.css";

// Route on the Tauri window label (SPEC §2.8: `popover` / `settings`). In a
// plain browser, show the dev page with both side by side.
const label = windowLabel();
const root =
  label === "popover" ? <Popover /> : label === "settings" ? <Settings /> : <DevPage />;

void startStore();

ReactDOM.createRoot(document.getElementById("root") as HTMLElement).render(
  <React.StrictMode>{root}</React.StrictMode>,
);
