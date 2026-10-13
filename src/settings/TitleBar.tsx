// Custom title bar for the Settings window on Windows (owner request E-1; the
// backend turns native decorations off there, SPEC 1.5.0). macOS keeps its
// native title bar, so this renders only on Windows.
//
// Drag and double-click are handled here with the window API rather than
// `data-tauri-drag-region`, so a double-click toggles maximize exactly once and
// only the permissions granted in SPEC 1.5.0 are used.
import { useEffect, useState, type MouseEvent, type ReactNode } from "react";
import { windowControls as W } from "../api/client";
import { APP_NAME } from "../lib/brand";

export function TitleBar() {
  const [maximized, setMaximized] = useState(false);

  useEffect(() => {
    let off: (() => void) | undefined;
    const sync = () => void W.isMaximized().then(setMaximized, () => {});
    sync();
    void W.onResized(sync).then((u) => (off = u));
    return () => off?.();
  }, []);

  const onBarMouseDown = (e: MouseEvent) => {
    if (e.button !== 0) return;
    if (e.detail === 2) void W.toggleMaximize();
    else void W.startDragging();
  };

  return (
    <header
      onMouseDown={onBarMouseDown}
      className="flex h-8 shrink-0 items-center justify-between border-b border-line bg-paper select-none"
      aria-label="Window title bar"
    >
      <span className="label pointer-events-none pl-4 text-ink">
        {APP_NAME} <span className="text-ink-muted">— Settings</span>
      </span>
      <div className="flex h-full" onMouseDown={(e) => e.stopPropagation()}>
        <Control label="Minimize" onClick={() => void W.minimize()}>
          <path d="M0.5 5.5h10" />
        </Control>
        <Control label={maximized ? "Restore" : "Maximize"} onClick={() => void W.toggleMaximize()}>
          {maximized ? (
            <>
              <rect x="0.5" y="2.5" width="8" height="8" />
              <path d="M2.5 2.5v-2h8v8h-2" />
            </>
          ) : (
            <rect x="0.5" y="0.5" width="10" height="10" />
          )}
        </Control>
        <Control label="Close" danger onClick={() => void W.close()}>
          <path d="M0.5 0.5l10 10M10.5 0.5l-10 10" />
        </Control>
      </div>
    </header>
  );
}

function Control({ label, onClick, danger = false, children }: { label: string; onClick: () => void; danger?: boolean; children: ReactNode }) {
  return (
    <button
      type="button"
      aria-label={label}
      title={label}
      onClick={onClick}
      className={`group grid h-full w-[46px] place-items-center text-ink transition-colors duration-fast focus-visible:outline-2 focus-visible:-outline-offset-2 focus-visible:outline-cobalt ${
        danger ? "hover:bg-vermilion hover:text-paper" : "hover:bg-ink/8"
      }`}
    >
      <svg width="11" height="11" viewBox="0 0 11 11" fill="none" stroke="currentColor" strokeWidth="1" aria-hidden>
        {children}
      </svg>
    </button>
  );
}
