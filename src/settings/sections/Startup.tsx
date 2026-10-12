// 07 / STARTUP — launch at login, auto-connect, close to tray (SPEC §2.6).
import { commands } from "../../api/client";
import type { AppSettings } from "../../api/bindings";
import { useApp } from "../../state/store";
import { protocolBadge } from "../../lib/format";
import { ErrorNote, SectionHeader, SettingRow, Switch } from "../kit";
import { useResource } from "../useResource";
import { APP_NAME } from "../../lib/brand";

export function StartupSection() {
  const r = useResource<AppSettings>(commands.getSettings, commands.setSettings);
  const status = useApp((s) => s.status);
  const nodes = useApp((s) => s.nodes);
  const s = r.saved;
  const set = (patch: Partial<AppSettings>) => s && void r.save({ ...s, ...patch });
  const node = nodes.find((n) => n.id === status?.selectedNodeId);
  const mode = status ? `${status.captureMode === "tun" ? "TUN" : "system proxy"} · ${status.routingMode} routing` : "";

  return (
    <div className="flex flex-col gap-5">
      <SectionHeader index="07 / Startup" title="Startup" />
      {r.error && <ErrorNote error={r.error} />}
      {s && (
        <div className="flex flex-col">
          <SettingRow title="Launch at login" control={<Switch label="Launch at login" checked={s.launchAtLogin} onChange={(v) => set({ launchAtLogin: v })} />}>
            Start {APP_NAME} in the menu bar when you log in.
          </SettingRow>
          <SettingRow title="Connect automatically" control={<Switch label="Connect automatically" checked={s.autoConnect} onChange={(v) => set({ autoConnect: v })} />}>
            When {APP_NAME} starts, connect with the last node and modes
            {node ? ` — now ${protocolBadge(node.protocol)} · ${node.name}, ${mode}` : ""}.
          </SettingRow>
          <SettingRow title="Keep running when the window is closed" control={<Switch label="Keep running when closed" checked={s.closeToTray} onChange={(v) => set({ closeToTray: v })} />}>
            {s.closeToTray
              ? `Closing the settings window leaves ${APP_NAME} running in the menu bar. Quit from the popover or the menu-bar menu.`
              : `Closing the settings window quits ${APP_NAME} — it disconnects and restores your network settings first.`}
          </SettingRow>
        </div>
      )}
    </div>
  );
}
