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

export type { SettingsSection } from "./bindings";

export interface Backend {
  kind: "mock" | "tauri";
  commands: Commands;
  on<K extends EventName>(name: K, cb: EventHandler<K>): Promise<Unlisten>;
}
