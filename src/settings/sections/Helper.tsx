// 06 / HELPER — privileged helper for TUN: status, install, reinstall, uninstall (SPEC §2.5, §9.3, 1.3.2).
import { useState } from "react";
import { commands } from "../../api/client";
import type { AppError } from "../../api/bindings";
import { selectTunUnavailable, useApp } from "../../state/store";
import { isProxying } from "../../popover/view";
import { Button, ErrorNote, Note, SectionHeader } from "../kit";

export function HelperSection() {
  const helper = useApp((s) => s.helper);
  const status = useApp((s) => s.status);
  const unavailable = useApp(selectTunUnavailable);
  const [busy, setBusy] = useState<null | "install" | "uninstall">(null);
  const [error, setError] = useState<AppError | null>(null);
  const tunLive = !!status && status.captureMode === "tun" && isProxying(status);

  const act = async (kind: "install" | "uninstall") => {
    setBusy(kind);
    setError(null);
    const r = kind === "install" ? await commands.installHelper() : await commands.uninstallHelper();
    setBusy(null);
    if (r.status === "error") setError(r.error); // status itself arrives via helperStatusChanged
  };

  const state = !helper
    ? "Checking…"
    : !helper.installed
      ? "Not installed"
      : helper.needsUpgrade
        ? "Installed — needs reinstalling"
        : helper.running
          ? "Installed and running"
          : "Installed, not responding";

  if (unavailable)
    return (
      <div className="flex flex-col gap-5">
        <SectionHeader index="06 / Helper" title="Helper" />
        <Note tone="info" title="Not available on Windows yet">
          The helper and TUN (whole-device) mode are coming later for Windows. Use System proxy mode for now — it covers apps that use the
          system proxy setting.
        </Note>
      </div>
    );

  return (
    <div className="flex flex-col gap-5">
      <SectionHeader index="06 / Helper" title="Helper" />
      <p className="text-body">
        TUN mode protects the whole device. It needs a small helper that runs with administrator rights: it starts the proxy core with a
        TUN interface, applies the kill switch and puts your network settings back. It is installed once and can be removed here.
      </p>

      <dl className="grid grid-cols-[120px_1fr] gap-x-4 gap-y-1.5 text-secondary">
        <dt className="label pt-px text-ink-muted">Status</dt>
        <dd className={helper?.installed && helper.running && !helper.needsUpgrade ? "text-cobalt" : helper?.installed ? "text-mustard-text" : ""}>{state}</dd>
        {helper?.installed && (
          <>
            <dt className="label pt-px text-ink-muted">Version</dt>
            <dd className="font-mono text-[12px]">
              {helper.version ?? "unknown"}
              {helper.needsUpgrade && <span className="text-ink-muted"> · app expects {helper.expectedVersion}</span>}
            </dd>
          </>
        )}
      </dl>

      {(!helper?.installed || helper.needsUpgrade) && (
        <Note tone="info" title="What to expect">
          <ul className="flex list-disc flex-col gap-0.5 pl-4">
            <li>macOS asks for your administrator password (“EdgeClient wants to make changes”). On Windows, a UAC prompt appears.</li>
            <li>macOS may show a “Background Items Added” notification — that is the helper.</li>
            <li>Nothing about your network changes until you connect in TUN mode.</li>
          </ul>
        </Note>
      )}
      {helper?.installed && !helper.running && !helper.needsUpgrade && (
        <Note tone="warn" title="The helper is installed but not responding">
          TUN connections will fail. Reinstalling usually fixes this.
        </Note>
      )}

      <div className="flex flex-wrap items-center gap-2">
        {(!helper?.installed || helper.needsUpgrade || !helper.running) && (
          <Button variant="solid" busy={busy === "install"} onClick={() => void act("install")}>
            {busy === "install" ? "Waiting for administrator approval…" : helper?.installed ? "Reinstall helper" : "Install helper"}
          </Button>
        )}
        {helper?.installed && (
          <Button busy={busy === "uninstall"} disabled={tunLive} onClick={() => void act("uninstall")} title={tunLive ? "Disconnect TUN first" : undefined}>
            {busy === "uninstall" ? "Waiting for administrator approval…" : "Uninstall helper"}
          </Button>
        )}
      </div>
      {tunLive && helper?.installed && <p className="text-secondary text-ink-muted">Disconnect the TUN session before uninstalling.</p>}
      {error &&
        (error.code === "HELPER_INSTALL_CANCELLED" ? (
          <Note title="Installation cancelled">Nothing was changed. You can try again any time.</Note>
        ) : (
          <ErrorNote error={error} />
        ))}
    </div>
  );
}
