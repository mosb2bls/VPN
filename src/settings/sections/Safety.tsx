// 04 / SAFETY — kill switch, direct fallback (warn before enabling), LAN bypass,
// Restore networking (SPEC §2.3, §9.4, §9.5, §9.8).
import { useState } from "react";
import { commands } from "../../api/client";
import type { RestoreReport, SafetySettings } from "../../api/bindings";
import { Button, ErrorNote, Note, SectionHeader, SettingRow, Switch } from "../kit";
import { useResource } from "../useResource";
import { APP_NAME } from "../../lib/brand";

export function SafetySection() {
  const r = useResource<SafetySettings>(commands.getSafetySettings, commands.setSafetySettings);
  const [confirmFallback, setConfirmFallback] = useState(false);
  const [restoring, setRestoring] = useState(false);
  const [restore, setRestore] = useState<{ ok: RestoreReport } | null>(null);
  const s = r.saved;
  const set = (patch: Partial<SafetySettings>) => s && void r.save({ ...s, ...patch });

  const restoreNow = async () => {
    setRestoring(true);
    setRestore(null);
    const res = await commands.restoreNetworking();
    setRestoring(false);
    if (res.status === "ok") setRestore({ ok: res.data });
    else r.setError(res.error); // RESTORE_INCOMPLETE details live in Logs › Last restore
  };

  return (
    <div className="flex flex-col gap-5">
      <SectionHeader index="04 / Safety" title="Safety" />
      {r.error && <ErrorNote error={r.error} />}
      {s && (
        <div className="flex flex-col">
          <SettingRow
            title="Kill switch"
            control={<Switch label="Kill switch" checked={s.killSwitch} onChange={(v) => set({ killSwitch: v })} />}
          >
            TUN mode only. While the proxy is restarting or has failed, traffic is blocked on purpose so nothing leaks. China and local
            traffic may also pause briefly. Recommended on.
          </SettingRow>
          {!s.killSwitch && <Note tone="warn" title="With the kill switch off, traffic can leak while the proxy restarts." />}

          <SettingRow
            title="Direct fallback"
            tone={s.directFallback ? "warn" : undefined}
            control={
              <Switch
                label="Direct fallback"
                checked={s.directFallback}
                onChange={(v) => (v ? setConfirmFallback(true) : set({ directFallback: false }))}
              />
            }
          >
            If every node fails, send traffic out directly instead of blocking it, and switch back to the proxy as soon as a node recovers.
            Off by default (fail closed). A change takes effect the next time you connect.
          </SettingRow>
          {confirmFallback && (
            <Note tone="error" title="Turn on direct fallback?">
              <p>When all nodes fail, your traffic will go out directly — unprotected and visible to your network — without asking.</p>
              <div className="mt-2 flex gap-2">
                <Button
                  onClick={() => {
                    setConfirmFallback(false);
                    set({ directFallback: true });
                  }}
                >
                  Turn on anyway
                </Button>
                <Button variant="ghost" onClick={() => setConfirmFallback(false)}>
                  Keep it off
                </Button>
              </div>
            </Note>
          )}
          {s.directFallback && !confirmFallback && <Note tone="warn" title="Direct fallback is on: if all nodes fail, traffic goes out unprotected." />}

          <SettingRow
            title="Local network bypass"
            control={<Switch label="Local network bypass" checked={s.allowLan} onChange={(v) => set({ allowLan: v })} />}
          >
            Printers, NAS, routers and other devices on your network (and CGNAT ranges such as Tailscale) are reached directly, not
            through the proxy.
          </SettingRow>
        </div>
      )}

      <section className="flex flex-col gap-3 border-t border-line pt-5">
        <h3 className="label">Restore networking</h3>
        <p className="text-secondary text-ink">
          Puts your system proxy, routes, DNS, IPv6 and firewall back the way they were before {APP_NAME} changed them, and stops the
          proxy. Safe to use at any time.
        </p>
        <div>
          <Button onClick={() => void restoreNow()} busy={restoring}>
            {restoring ? "Restoring…" : "Restore networking"}
          </Button>
        </div>
        {restore && <Note tone="ok" title="Network settings restored" />}
        <p className="text-secondary text-ink-muted">
          If the app cannot start, run it from a terminal with <code className="font-mono text-[11px] text-ink">--restore-networking</code>.
        </p>
      </section>
    </div>
  );
}
