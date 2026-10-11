// Shared API-layer types derived from the generated bindings, so the client
// and the mock can never drift from SPEC.
import type { AppError, commands, events } from "./bindings";

export type Result<T> = { status: "ok"; data: T } | { status: "error"; error: AppError };

export type Commands = typeof commands;
export type CommandName = keyof Commands;

type Events = typeof events;
export type EventName = keyof Events;
/** Payload type of each SPEC event, keyed by its camelCase name. */
export type EventMap = {
  [K in EventName]: Parameters<Parameters<Events[K]["listen"]>[0]>[0]["payload"];
};
export type EventHandler<K extends EventName> = (payload: EventMap[K]) => void;
export type Unlisten = () => void;

// PENDING REQ-1 — not in SPEC yet (backend step B1.2 → SPEC v0.3.0).
// Shape as proposed in REQ-1; replace with generated types once it lands.
export type SettingsSection =
  | "nodes"
  | "connection"
  | "failover"
  | "safety"
  | "rules"
  | "helper"
  | "startup"
  | "logs"
  | "about";

// PENDING REQ-1
export interface PendingWindowApi {
  openSettings(section: SettingsSection | null): Promise<Result<null>>;
  hidePopover(): Promise<Result<null>>;
  onSettingsSectionRequested(cb: (section: SettingsSection) => void): Promise<Unlisten>;
  onPopoverShown(cb: () => void): Promise<Unlisten>;
}

export interface Backend {
  kind: "mock" | "tauri";
  commands: Commands;
  on<K extends EventName>(name: K, cb: EventHandler<K>): Promise<Unlisten>;
  /** PENDING REQ-1 */
  pending: PendingWindowApi;
}
