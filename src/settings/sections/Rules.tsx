// 05 / RULES — bundled/downloaded rule-sets, auto-update, update now (SPEC §2.4, 1.3.0).
import { useEffect, useState } from "react";
import { commands, on } from "../../api/client";
import type { AppError, AppSettings, RulesStatus } from "../../api/bindings";
import { bytes } from "../../lib/format";
import { Button, ErrorNote, Field, Note, SectionHeader, SettingRow, Switch, TextInput, fieldErrorFor } from "../kit";
import { useResource } from "../useResource";

const when = (iso: string | null) => (iso ? new Date(iso).toLocaleString() : "—");

export function RulesSection() {
  const [rules, setRules] = useState<RulesStatus | null>(null);
  const [busy, setBusy] = useState(false);
  const [result, setResult] = useState<{ changed: boolean } | null>(null);
  const [error, setError] = useState<AppError | null>(null);
  const app = useResource<AppSettings>(commands.getSettings, commands.setSettings);
  const [hours, setHours] = useState<string>("");

  useEffect(() => {
    void commands.getRulesStatus().then((r) => (r.status === "ok" ? setRules(r.data) : setError(r.error)));
    let off: (() => void) | undefined;
    void on("rulesUpdated", (p) => setRules(p.status)).then((u) => (off = u));
    return () => off?.();
  }, []);
  useEffect(() => {
    if (app.saved) setHours(String(app.saved.rulesUpdateIntervalHours));
  }, [app.saved]);

  const update = async () => {
    setBusy(true);
    setError(null);
    setResult(null);
    const before = rules?.version;
    const r = await commands.updateRulesNow();
    setBusy(false);
    if (r.status === "ok") {
      setRules(r.data);
      setResult({ changed: r.data.version !== before });
    } else setError(r.error);
  };

  return (
    <div className="flex flex-col gap-5">
      <SectionHeader index="05 / Rules" title="Rules" />
      <p className="text-secondary text-ink-muted">Rule mode sends China sites and addresses directly and everything else through the proxy, using these rule-sets.</p>

      {rules && (
        <section className="flex flex-col gap-3">
          <dl className="grid grid-cols-[120px_1fr] gap-x-4 gap-y-1.5 text-secondary">
            <dt className="label pt-px text-ink-muted">Version</dt>
            <dd className="font-mono text-[12px]">
              {rules.version} · {rules.source === "bundled" ? "bundled with the app" : "downloaded"}
            </dd>
            <dt className="label pt-px text-ink-muted">Rules dated</dt>
            <dd>{when(rules.updatedAt)}</dd>
            <dt className="label pt-px text-ink-muted">Last check</dt>
            <dd>{when(rules.lastCheckAt)}</dd>
            <dt className="label pt-px text-ink-muted">Next check</dt>
            <dd>{app.saved?.rulesAutoUpdate === false ? "Automatic updates are off" : when(rules.nextCheckAt)}</dd>
          </dl>
          <table className="w-full max-w-[460px] text-secondary">
            <thead>
              <tr className="label text-left text-ink-muted">
                <th className="py-1 font-normal">Set</th>
                <th className="py-1 font-normal">Size</th>
                <th className="py-1 font-normal">SHA-256</th>
              </tr>
            </thead>
            <tbody>
              {rules.sets.map((s) => (
                <tr key={s.name} className="border-t border-line">
                  <td className="py-1.5 font-mono text-[12px]">{s.name}</td>
                  <td className="py-1.5 tabular">{bytes(s.sizeBytes)}</td>
                  <td className="py-1.5 font-mono text-[12px] text-ink-muted">{s.sha256Prefix}…</td>
                </tr>
              ))}
            </tbody>
          </table>
          {rules.lastError && (
            <ErrorNote
              error={rules.lastError}
              title={rules.lastError.code === "RULES_MISSING" ? "Routing rules are missing or damaged" : "The last rule update failed"}
            />
          )}
          {rules.lastError?.code === "RULES_MISSING" && <p className="text-secondary text-ink-muted">Connecting will fail until the rules are restored. Reinstall the app.</p>}
        </section>
      )}

      <div className="flex items-center gap-3">
        <Button onClick={() => void update()} busy={busy || !!rules?.updating}>
          {busy || rules?.updating ? "Updating…" : "Update now"}
        </Button>
        {result && <span className="text-secondary">{result.changed ? `Updated to ${rules?.version}.` : "Already up to date."}</span>}
      </div>
      {error && <ErrorNote error={error} />}

      {app.saved && (
        <div className="flex flex-col border-t border-line">
          <SettingRow
            title="Update rules automatically"
            control={<Switch label="Update rules automatically" checked={app.saved.rulesAutoUpdate} onChange={(v) => void app.save({ ...app.saved!, rulesAutoUpdate: v })} />}
          >
            Checks the rule sources in the background. If an update fails, the current rules stay in use.
          </SettingRow>
          {app.saved.rulesAutoUpdate && (
            <form
              className="flex items-end gap-3 pb-2"
              onSubmit={(e) => {
                e.preventDefault();
                void app.save({ ...app.saved!, rulesUpdateIntervalHours: Number(hours) });
              }}
            >
              <Field label="Check every (hours)" error={fieldErrorFor(app.error, "rulesUpdateIntervalHours")} htmlFor="r-hours" help="1–168 hours.">
                <TextInput id="r-hours" mono inputMode="numeric" className="w-24" value={hours} onChange={(e) => setHours(e.target.value)} invalid={!!fieldErrorFor(app.error, "rulesUpdateIntervalHours")} />
              </Field>
              {hours !== String(app.saved.rulesUpdateIntervalHours) && (
                <Button type="submit" variant="solid" busy={app.busy} className="mb-[22px]">
                  Save
                </Button>
              )}
            </form>
          )}
          {app.error && !app.error.fields.length && <ErrorNote error={app.error} />}
        </div>
      )}
      {!rules && !error && <Note title="Loading rule status…" />}
    </div>
  );
}
