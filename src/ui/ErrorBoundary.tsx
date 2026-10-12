// Last line of defence: a render error must never leave a window blank. Shows
// what happened and the safe ways out (reload, restore networking, quit).
import { Component, type ErrorInfo, type ReactNode } from "react";
import { commands } from "../api/client";
import { APP_NAME } from "../lib/brand";

export class ErrorBoundary extends Component<{ children: ReactNode; compact?: boolean }, { error: Error | null }> {
  state = { error: null as Error | null };

  static getDerivedStateFromError(error: Error) {
    return { error };
  }

  componentDidCatch(error: Error, info: ErrorInfo) {
    console.error("UI render error", error, info.componentStack);
  }

  render() {
    if (!this.state.error) return this.props.children;
    return (
      <div role="alert" className="paper-grain flex h-full flex-col justify-center gap-4 p-6 text-ink">
        <span className="label text-ink-muted">{APP_NAME}</span>
        <h1 className="font-display text-[26px] leading-8 font-[850] [font-stretch:82%] text-vermilion uppercase">Display error</h1>
        <p className="text-body">
          This window hit an error and stopped drawing. Your connection is not affected by this — the app keeps running in the
          background.
        </p>
        <p className="font-mono text-[11px] break-all text-ink-muted">{this.state.error.message}</p>
        <div className="flex flex-wrap gap-2">
          <button className="label h-8 border border-ink bg-ink px-3 text-paper" onClick={() => location.reload()}>
            Reload window
          </button>
          <button className="label h-8 border border-ink px-3" onClick={() => void commands.restoreNetworking()}>
            Restore networking
          </button>
          <button className="label h-8 border border-ink px-3" onClick={() => void commands.quitApp()}>
            Quit
          </button>
        </div>
      </div>
    );
  }
}
