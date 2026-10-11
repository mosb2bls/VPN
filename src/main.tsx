import React from "react";
import ReactDOM from "react-dom/client";
import { windowLabel } from "./api/client";
import { DevPage } from "./dev/DevPage";
import { PopoverDebug } from "./dev/PopoverDebug";
import { SettingsDebug } from "./dev/SettingsDebug";
import { startStore } from "./state/store";
import "./design/tokens.css";
import "./dev/dev.css";

// Route on the Tauri window label (REQ-1: `popover` / `settings`). In a plain
// browser, or the current single `main` window, show the dev page.
const label = windowLabel();
const root =
  label === "popover" ? <PopoverDebug /> : label === "settings" ? <SettingsDebug /> : <DevPage />;

void startStore();

ReactDOM.createRoot(document.getElementById("root") as HTMLElement).render(
  <React.StrictMode>{root}</React.StrictMode>,
);
