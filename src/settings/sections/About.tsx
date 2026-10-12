// 09 / ABOUT — versions (app, SPEC, core, rules, helper), platform, licences.
import { useEffect, useState } from "react";

/** Licence text from the app bundle (public/licenses), loaded when opened. */
function Licence({ name, file }: { name: string; file: string }) {
  const [text, setText] = useState<string | null>(null);
  return (
    <details
      onToggle={(e) => {
        if ((e.currentTarget as HTMLDetailsElement).open && text === null)
          void fetch(`/licenses/${file}`)
            .then((r) => r.text())
            .then(setText, () => setText("Licence file not found."));
      }}
    >
      <summary className="label cursor-pointer text-ink-muted">{name} licence</summary>
      <pre className="mt-2 max-h-[200px] overflow-y-auto border border-line bg-paper-raised p-3 font-mono text-[10px] leading-[15px] whitespace-pre-wrap">
        {text ?? "Loading…"}
      </pre>
    </details>
  );
}
import { commands } from "../../api/client";
import type { AppInfo, RulesStatus } from "../../api/bindings";
import { useApp } from "../../state/store";
import { SectionHeader } from "../kit";

export function AboutSection() {
  const [info, setInfo] = useState<AppInfo | null>(null);
  const [rules, setRules] = useState<RulesStatus | null>(null);
  const helper = useApp((s) => s.helper);
  useEffect(() => {
    void commands.getAppInfo().then((r) => r.status === "ok" && setInfo(r.data));
    void commands.getRulesStatus().then((r) => r.status === "ok" && setRules(r.data));
  }, []);
  const rows: [string, string][] = info
    ? [
        ["App", `${info.appName} ${info.appVersion}`],
        ["Interface (SPEC)", info.specVersion],
        ["Proxy core", info.coreVersion ? `sing-box ${info.coreVersion}` : "not bundled"],
        ["Rules", rules ? `${rules.version} (${rules.source})` : "—"],
        ["Helper", helper?.installed ? (helper.version ?? "installed") : "not installed"],
        ["Platform", info.platform === "macos" ? "macOS" : info.platform === "windows" ? "Windows" : "other"],
      ]
    : [];
  return (
    <div className="flex flex-col gap-6">
      <SectionHeader index="09 / About" title="About" />
      <dl className="grid grid-cols-[150px_1fr] gap-x-4 gap-y-2 text-secondary">
        {rows.map(([k, v]) => (
          <div key={k} className="contents">
            <dt className="label pt-px text-ink-muted">{k}</dt>
            <dd className="font-mono text-[12px]">{v}</dd>
          </div>
        ))}
      </dl>
      <section className="flex flex-col gap-2 border-t border-line pt-5 text-secondary">
        <h3 className="label">Privacy</h3>
        <p>
          By design EdgeClient sends no telemetry. It connects only to your own server, DNS resolvers, health-check targets and rule-set
          sources.
        </p>
      </section>
      <section className="flex flex-col gap-2 border-t border-line pt-5 text-secondary">
        <h3 className="label">Fonts</h3>
        <p>Archivo and IBM Plex Mono, bundled with the app under the SIL Open Font License 1.1.</p>
        <Licence name="Archivo" file="Archivo-OFL.txt" />
        <Licence name="IBM Plex Mono" file="IBMPlexMono-OFL.txt" />
      </section>
    </div>
  );
}
