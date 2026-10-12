// 03 / FAILOVER — automatic switching between HY2 and REALITY (SPEC §2.3, §5, §9.7).
import { useState } from "react";
import { commands } from "../../api/client";
import type { FailoverSettings } from "../../api/bindings";
import { protocolBadge } from "../../lib/format";
import { useApp } from "../../state/store";
import { Button, DurationInput, ErrorNote, Field, Note, SectionHeader, SettingRow, Switch, TextInput, fieldErrorFor } from "../kit";
import { useResource } from "../useResource";

/** SPEC §2.3 defaults. */
const DEFAULTS: FailoverSettings = {
  enabled: true,
  fallbackNodeId: null,
  probeTargets: ["https://www.gstatic.com/generate_204", "https://cp.cloudflare.com/"],
  probeIntervalSecs: 10,
  probeTimeoutSecs: 5,
  failureRounds: 3,
  minDwellSecs: 300,
  returnStableSecs: 600,
  maxSwitchesPerHour: 4,
};

const human = (secs: number) =>
  secs % 3600 === 0 && secs >= 3600 ? `${secs / 3600} h` : secs % 60 === 0 && secs >= 60 ? `${secs / 60} min` : `${secs} s`;

export function FailoverSection() {
  const r = useResource<FailoverSettings>(commands.getFailoverSettings, commands.setFailoverSettings);
  const nodes = useApp((s) => s.nodes);
  const activity = useApp((s) => s.activity);
  const [targets, setTargets] = useState<string | null>(null);
  const d = r.draft;
  if (!d) return <SectionHeader index="03 / Failover" title="Failover" />;
  const set = (patch: Partial<FailoverSettings>) => r.setDraft({ ...d, ...patch });
  const fe = (f: string) => fieldErrorFor(r.error, f);
  const targetText = targets ?? d.probeTargets.join("\n");
  const commit = () =>
    void r.save({
      ...d,
      probeTargets: targetText
        .split(/\n/)
        .map((t) => t.trim())
        .filter(Boolean),
    });
  const dirty = r.dirty || (targets !== null && targets !== r.saved?.probeTargets.join("\n"));
  const fallback = nodes.find((n) => n.id === d.fallbackNodeId);
  const reasons = activity.filter((a) => /probe|fallback|primary|switch|failover|direct|all nodes/i.test(a.text)).slice(0, 8);

  return (
    <div className="flex flex-col gap-5">
      <SectionHeader index="03 / Failover" title="Failover" />
      <p className="text-secondary text-ink">
        HY2 uses UDP, which some networks block or slow down. EdgeClient checks the connection through the active node and, if it keeps
        failing, switches to the fallback node (usually REALITY over TCP 443). Choosing a node yourself always overrides this.
      </p>

      <SettingRow
        title="Automatic failover"
        control={<Switch label="Automatic failover" checked={d.enabled} onChange={(v) => void r.save({ ...(r.saved ?? d), enabled: v })} />}
      >
        {d.enabled
          ? `After ${d.failureRounds} failed checks in a row (one every ${human(d.probeIntervalSecs)}, about ${human(d.failureRounds * d.probeIntervalSecs)}), switch to ${
              fallback ? `“${fallback.name}”` : "the first node of the other protocol"
            }. Return to the primary once it has passed for ${human(d.returnStableSecs)}.`
          : "Off — when the active node fails, the popover shows DEGRADED and you switch nodes yourself."}
      </SettingRow>

      {d.enabled && (
        <form
          className="flex flex-col gap-4"
          onSubmit={(e) => {
            e.preventDefault();
            commit();
          }}
        >
          <Field label="Fallback node" error={fe("fallbackNodeId")} htmlFor="f-fb">
            <select
              id="f-fb"
              value={d.fallbackNodeId ?? ""}
              onChange={(e) => set({ fallbackNodeId: e.target.value || null })}
              className="h-8 max-w-[360px] border border-line bg-paper-raised px-2 text-body focus-visible:outline-2 focus-visible:outline-cobalt"
            >
              <option value="">Automatic — first node of the other protocol</option>
              {nodes.map((n) => (
                <option key={n.id} value={n.id}>
                  {protocolBadge(n.protocol)} · {n.name}
                </option>
              ))}
            </select>
          </Field>

          <div className="grid grid-cols-2 gap-x-6 gap-y-4">
            <Field label="Check every" error={fe("probeIntervalSecs")} htmlFor="f-int" help="5 s – 5 min.">
              <DurationInput id="f-int" value={d.probeIntervalSecs} onChange={(v) => set({ probeIntervalSecs: v })} invalid={!!fe("probeIntervalSecs")} units={["s", "min"]} />
            </Field>
            <Field label="Check timeout" error={fe("probeTimeoutSecs")} htmlFor="f-to" help="1–30 s, shorter than the interval.">
              <DurationInput id="f-to" value={d.probeTimeoutSecs} onChange={(v) => set({ probeTimeoutSecs: v })} invalid={!!fe("probeTimeoutSecs")} units={["s"]} />
            </Field>
            <Field label="Failed checks before switching" error={fe("failureRounds")} htmlFor="f-rounds" help="1–20.">
              <TextInput id="f-rounds" mono inputMode="numeric" className="w-20" value={String(d.failureRounds)} onChange={(e) => set({ failureRounds: Number(e.target.value) })} invalid={!!fe("failureRounds")} />
            </Field>
            <Field label="Stay at least" error={fe("minDwellSecs")} htmlFor="f-dwell" help="Minimum time on a node after any switch (0 – 1 h).">
              <DurationInput id="f-dwell" value={d.minDwellSecs} onChange={(v) => set({ minDwellSecs: v })} invalid={!!fe("minDwellSecs")} />
            </Field>
            <Field label="Return to primary after" error={fe("returnStableSecs")} htmlFor="f-ret" help="Primary must pass this long (1 min – 24 h).">
              <DurationInput id="f-ret" value={d.returnStableSecs} onChange={(v) => set({ returnStableSecs: v })} invalid={!!fe("returnStableSecs")} units={["min", "h"]} />
            </Field>
            <Field label="Max switches per hour" error={fe("maxSwitchesPerHour")} htmlFor="f-max" help="1–60. Stops flapping on bad networks.">
              <TextInput id="f-max" mono inputMode="numeric" className="w-20" value={String(d.maxSwitchesPerHour)} onChange={(e) => set({ maxSwitchesPerHour: Number(e.target.value) })} invalid={!!fe("maxSwitchesPerHour")} />
            </Field>
          </div>

          <Field
            label="Check targets"
            error={fe("probeTargets")}
            htmlFor="f-targets"
            help="2–5 http(s) addresses that answer quickly (HTTP 204/200), on at least 2 different hosts. A check fails only if all of them fail."
          >
            <textarea
              id="f-targets"
              rows={3}
              spellCheck={false}
              value={targetText}
              onChange={(e) => setTargets(e.target.value)}
              aria-invalid={!!fe("probeTargets") || undefined}
              className={`w-full max-w-[460px] border bg-paper-raised px-2 py-1.5 font-mono text-[12px] leading-[18px] outline-none focus:border-ink focus-visible:outline-2 focus-visible:outline-cobalt ${
                fe("probeTargets") ? "border-vermilion" : "border-line"
              }`}
            />
          </Field>

          {r.error && r.error.code !== "VALIDATION_FAILED" && <ErrorNote error={r.error} />}
          {r.error?.code === "VALIDATION_FAILED" && <Note tone="error" title={r.error.message} />}
          <div className="flex gap-2">
            <Button type="submit" variant="solid" disabled={!dirty} busy={r.busy}>
              Save
            </Button>
            <Button
              variant="ghost"
              disabled={!dirty}
              onClick={() => {
                r.setDraft(r.saved);
                setTargets(null);
                r.setError(null);
              }}
            >
              Revert
            </Button>
            <Button
              variant="ghost"
              onClick={() => {
                r.setDraft({ ...DEFAULTS, fallbackNodeId: d.fallbackNodeId });
                setTargets(null);
              }}
            >
              Defaults
            </Button>
          </div>
        </form>
      )}

      <section className="flex flex-col gap-2 border-t border-line pt-5">
        <h3 className="label">Recent failover events</h3>
        {reasons.length === 0 ? (
          <p className="text-secondary text-ink-muted">None since EdgeClient started.</p>
        ) : (
          <ul className="flex flex-col gap-1">
            {reasons.map((a) => (
              <li key={a.id} className="grid grid-cols-[64px_1fr] gap-3 text-secondary">
                <span className="label pt-px text-ink-muted tabular">{new Date(a.at).toLocaleTimeString([], { hour: "2-digit", minute: "2-digit" })}</span>
                <span className={a.tone === "warn" ? "text-mustard-text" : a.tone === "error" ? "text-vermilion-text" : ""}>{a.text}</span>
              </li>
            ))}
          </ul>
        )}
      </section>
    </div>
  );
}
