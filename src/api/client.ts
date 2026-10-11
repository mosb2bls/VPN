// The only entry point components use to talk to the backend.
// Components never import `bindings.ts` values or call `invoke` directly.
//
// Mock vs real: inside Tauri the real backend is used, in a plain browser the
// mock. `VITE_USE_MOCK=1` / `VITE_USE_MOCK=0` overrides either way, e.g.
// `VITE_USE_MOCK=1 npm run tauri dev` runs the mock inside the real windows.
import { getCurrentWindow } from "@tauri-apps/api/window";
import { commands as tauriCommands, events as tauriEvents } from "./bindings";
import type { AppError } from "./bindings";
import { mockBackend } from "./mock";
import type { Backend, Commands, EventName, Result, Unlisten } from "./types";

export type * from "./types";

export const IN_TAURI = typeof window !== "undefined" && "__TAURI_INTERNALS__" in window;

const override = import.meta.env.VITE_USE_MOCK as string | undefined;
export const USE_MOCK = override === "1" ? true : override === "0" ? false : !IN_TAURI;

/** A thrown JS Error from bindings means the IPC bridge failed (SPEC §1). */
function bridgeError(e: unknown): AppError {
  return {
    code: "INTERNAL",
    message: "The app backend could not be reached.",
    recoverable: true,
    fields: [],
    detail: `IPC: ${e instanceof Error ? e.message : String(e)}`,
    restoreReport: null,
  };
}

// Wrap every real command so a bridge failure becomes a normal error result
// instead of an unhandled rejection.
const wrappedTauriCommands = Object.fromEntries(
  Object.entries(tauriCommands).map(([name, fn]) => [
    name,
    async (...args: unknown[]) => {
      try {
        return await (fn as (...a: unknown[]) => Promise<Result<unknown>>)(...args);
      } catch (e) {
        return { status: "error", error: bridgeError(e) };
      }
    },
  ]),
) as unknown as Commands;

const pendingNotImplemented = async (): Promise<Result<null>> => ({
  status: "error",
  error: {
    code: "NOT_IMPLEMENTED",
    message: "This feature is not implemented yet.",
    recoverable: true,
    fields: [],
    detail: "PENDING REQ-1: window commands are not in SPEC yet",
    restoreReport: null,
  },
});

const tauriBackend: Backend = {
  kind: "tauri",
  commands: wrappedTauriCommands,
  async on(name, cb) {
    const ev = tauriEvents[name as EventName] as unknown as {
      listen: (h: (e: { payload: unknown }) => void) => Promise<Unlisten>;
    };
    return ev.listen((e) => (cb as (p: unknown) => void)(e.payload));
  },
  // PENDING REQ-1 — no-ops until SPEC v0.3.0.
  pending: {
    openSettings: pendingNotImplemented,
    hidePopover: pendingNotImplemented,
    onSettingsSectionRequested: async () => () => {},
    onPopoverShown: async () => () => {},
  },
};

export const backend: Backend = USE_MOCK ? mockBackend : tauriBackend;
export const commands: Commands = backend.commands;
export const on: Backend["on"] = backend.on.bind(backend);
/** PENDING REQ-1 */
export const pendingWindow = backend.pending;

/** `popover` / `settings` inside Tauri (REQ-1), `null` in a plain browser. */
export function windowLabel(): string | null {
  if (!IN_TAURI) return null;
  try {
    return getCurrentWindow().label;
  } catch {
    return null;
  }
}

export function isNotImplemented(e: AppError | null | undefined): boolean {
  return e?.code === "NOT_IMPLEMENTED";
}
