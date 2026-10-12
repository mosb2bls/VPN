// 02 / CONNECTION — capture mode, routing mode, DNS profile (SPEC §2.2, §6, §9.1–3, 9.9).
import { useState } from "react";
import { commands } from "../../api/client";
import type { AppError, CaptureMode, DnsProfile, RoutingMode } from "../../api/bindings";
import type { Result } from "../../api/types";
import { applyStatus, selectTunLabel, useApp } from "../../state/store";
import { Segmented } from "../../ui/Segmented";
import { isProxying } from "../../popover/view";
import { Button, ErrorNote, Note, SectionHeader } from "../kit";
import { useGoTo } from "../nav";

export function ConnectionSection() {
  const status = useApp((s) => s.status);
  const helper = useApp((s) => s.helper);
  const tunLabel = useApp(selectTunLabel);
  const [error, setError] = useState<AppError | null>(null);
  const goTo = useGoTo();
  if (!status) return null;
  const live = isProxying(status);

  const run = async (p: Promise<Result<import("../../api/bindings").Status>>) => {
    setError(null);
    const r = await p;
    if (r.status === "ok") applyStatus(r.data);
    else setError(r.error);
  };

  return (
    <div className="flex flex-col gap-7">
      <SectionHeader index="02 / Connection" title="Connection" />
      {error && <ErrorNote error={error} />}

      <Block title="Capture" help="How traffic reaches EdgeClient.">
        <Segmented<CaptureMode>
          label="Capture mode"
          options={[
            { value: "system_proxy", label: "System proxy" },
            { value: "tun", label: "TUN — whole device", title: tunLabel ? `TUN (${tunLabel.toLowerCase()})` : undefined },
          ]}
          value={status.captureMode}
          onChange={(m) => void run(commands.setCaptureMode(m))}
        />
        {status.captureMode === "system_proxy" ? (
          <Note tone="warn" title="Only apps that use the system proxy are covered">
            Most browsers do; many command-line tools, games, updaters and system services do not. System proxy mode cannot stop IPv6
            bypass and has no kill switch — if the proxy stops, apps connect directly. Use TUN for whole-device protection.
          </Note>
        ) : (
          <Note tone="info" title="TUN covers the whole device">
            All traffic goes through EdgeClient. The kill switch blocks traffic while the proxy is down, so nothing leaks.
          </Note>
        )}
        {tunLabel && <span className="label text-ink">TUN · {tunLabel}</span>}
        {status.captureMode === "tun" && helper && (!helper.installed || helper.needsUpgrade) && (
          <Note tone="warn" title={helper.installed ? "The helper needs to be reinstalled" : "TUN needs the helper"}>
            <div className="flex items-center gap-3">
              <span>A one-time install with your administrator password.</span>
              <Button onClick={() => goTo("helper")}>Open Helper</Button>
            </div>
          </Note>
        )}
      </Block>

      <Block title="Routing">
        <Segmented<RoutingMode>
          label="Routing mode"
          options={[
            { value: "rule", label: "Rule" },
            { value: "global", label: "Global" },
            { value: "direct", label: "Direct" },
          ]}
          value={status.routingMode}
          onChange={(m) => void run(commands.setRoutingMode(m))}
        />
        <dl className="grid grid-cols-[72px_1fr] gap-x-3 gap-y-1.5 text-secondary">
          <Def term="Rule" on={status.routingMode === "rule"}>
            China and local network go direct; everything else through the proxy. Recommended.
          </Def>
          <Def term="Global" on={status.routingMode === "global"}>
            Everything through the proxy, except the local network and system essentials.
          </Def>
          <Def term="Direct" on={status.routingMode === "direct"}>
            No proxy at all — your traffic is not protected.
          </Def>
        </dl>
      </Block>

      <Block title="DNS profile" help="Where domain names are looked up.">
        <Segmented<DnsProfile>
          label="DNS profile"
          options={[
            { value: "mainland", label: "Mainland" },
            { value: "overseas", label: "Overseas" },
          ]}
          value={status.dnsProfile}
          onChange={(p) => void run(commands.setDnsProfile(p))}
        />
        <dl className="grid grid-cols-[72px_1fr] gap-x-3 gap-y-1.5 text-secondary">
          <Def term="Mainland" on={status.dnsProfile === "mainland"}>
            China sites are looked up directly with AliDNS (DoH, 223.5.5.5). Use this inside mainland China.
          </Def>
          <Def term="Overseas" on={status.dnsProfile === "overseas"}>
            Direct sites are looked up directly with Quad9 (DoH, 9.9.9.9). Use this outside China.
          </Def>
        </dl>
        <p className="text-secondary text-ink-muted">
          In both profiles, proxied sites are looked up through the proxy with Cloudflare DoH (1.1.1.1); local names (.local, .lan) use your
          system resolver. IPv6 addresses are not returned.
        </p>
      </Block>

      {live && (
        <p className="text-secondary text-ink-muted">
          While connected, changing capture mode reconnects. Routing and DNS changes restart the proxy core briefly
          {status.captureMode === "tun" ? "; the kill switch blocks traffic during the restart." : "; apps may connect directly for a moment."}
        </p>
      )}
    </div>
  );
}

function Block({ title, help, children }: { title: string; help?: string; children: React.ReactNode }) {
  return (
    <section className="flex flex-col gap-3">
      <div className="flex items-baseline justify-between">
        <h3 className="label">{title}</h3>
        {help && <span className="text-secondary text-ink-muted">{help}</span>}
      </div>
      <div className="max-w-[420px]">{(children as React.ReactNode[])[0]}</div>
      {(children as React.ReactNode[]).slice(1)}
    </section>
  );
}

function Def({ term, on, children }: { term: string; on: boolean; children: React.ReactNode }) {
  return (
    <>
      <dt className={`label pt-px ${on ? "text-ink" : "text-ink-muted"}`}>{term}</dt>
      <dd className={on ? "text-ink" : "text-ink-muted"}>{children}</dd>
    </>
  );
}
