// Dev page: popover and settings window side by side at their real sizes,
// plus the mock state switcher. `npm run dev` → http://localhost:1420
import { useEffect, useState } from "react";
import { SPEC_VERSION } from "../api/bindings";
import { USE_MOCK } from "../api/client";
import { mockEngine } from "../api/mock";
import { useApp } from "../state/store";
import { DevPanel } from "./DevPanel";
import { Popover } from "../popover/Popover";
import { PopoverDebug } from "./PopoverDebug";
import { SettingsDebug } from "./SettingsDebug";
import { TokenSheet } from "./TokenSheet";
import "./dev.css";

type Tab = "windows" | "tokens";
const tabFromHash = (): Tab => (location.hash === "#tokens" ? "tokens" : "windows");

export function DevPage() {
  const [tab, setTab] = useState<Tab>(tabFromHash);
  useEffect(() => {
    const h = () => setTab(tabFromHash());
    window.addEventListener("hashchange", h);
    return () => window.removeEventListener("hashchange", h);
  }, []);
  return (
    <>
      <nav className="dev-tabs">
        <a href="#windows" aria-current={tab === "windows"}>
          Windows
        </a>
        <a href="#tokens" aria-current={tab === "tokens"}>
          Tokens
        </a>
      </nav>
      {tab === "tokens" ? <TokenSheet /> : <Windows />}
    </>
  );
}

function Windows() {
  const [quit, setQuit] = useState(false);
  const [debug, setDebug] = useState(false);
  // Simulated tray: any backend hide (Esc, Settings…) sends popoverHidden; the tray button shows it again.
  const [hidden, setHidden] = useState(false);
  useEffect(
    () =>
      mockEngine.onAny((name) => {
        if (name === "popoverHidden") setHidden(true);
      }),
    [],
  );
  const trayClick = () => {
    setHidden(false);
    mockEngine.emitPopoverShown();
  };
  const bootError = useApp((s) => s.bootError);
  useEffect(() => {
    mockEngine.onQuit = () => setQuit(true);
  }, []);

  return (
    <div className="dev-page">
      <header className="dev-header">
        <span>EDGECLIENT · DEV</span>
        <span>SPEC {SPEC_VERSION}</span>
        <span className={USE_MOCK ? "dev-mock" : "dev-real"}>{USE_MOCK ? "MOCK BACKEND" : "REAL BACKEND"}</span>
        {bootError && (
          <span className="dev-real">
            startup error: {bootError.code} — {bootError.message}
          </span>
        )}
      </header>
      <div className="dev-windows">
        <figure>
          <figcaption>
            POPOVER · 340 × 480 ·{" "}
            <button className="dev-link" onClick={() => setDebug((d) => !d)}>
              {debug ? "show designed" : "show debug view"}
            </button>
            {USE_MOCK && !debug && (
              <>
                {" · "}
                <button className="dev-link" onClick={trayClick}>
                  replay open
                </button>
              </>
            )}
          </figcaption>
          <div className="dev-window" style={{ width: 340, height: 480 }}>
            {quit ? (
              <div className="dbg-pad">
                App quit (mock). <button onClick={() => location.reload()}>Relaunch</button>
              </div>
            ) : debug ? (
              <PopoverDebug />
            ) : (
              <>
                <div style={{ visibility: hidden ? "hidden" : "visible" }}>
                  <Popover />
                </div>
                {hidden && (
                  <button className="dev-tray" onClick={trayClick}>
                    Popover hidden.
                    <br />
                    Click to simulate the tray icon.
                  </button>
                )}
              </>
            )}
          </div>
        </figure>
        <figure>
          <figcaption>SETTINGS · 760 × 560</figcaption>
          <div className="dev-window" style={{ width: 760, height: 560 }}>
            <SettingsDebug />
          </div>
        </figure>
      </div>
      {USE_MOCK && <DevPanel />}
    </div>
  );
}
