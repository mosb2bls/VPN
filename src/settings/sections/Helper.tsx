// 06 / HELPER — privileged helper for TUN: status, install, reinstall, uninstall
// (SPEC §2.5, §9.3, 1.3.2; Windows service 1.4.2 — experimental, owner decision).
import { useState } from "react";
import { commands } from "../../api/client";
import type { AppError } from "../../api/bindings";
import { selectTunLabel, useApp } from "../../state/store";
import { isProxying } from "../../popover/view";
import { APP_NAME } from "../../lib/brand";
import { Button, ErrorNote, Note, SectionHeader } from "../kit";

export function HelperSection() {
  const helper = useApp((s) => s.helper);
  const status = useApp((s) => s.status);
  const tunLabel = useApp(selectTunLabel);
  const windows = useApp((s) => s.appInfo?.platform === "windows");
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

  return (
    <div className="flex flex-col gap-5">
      <SectionHeader index="06 / Helper" title="Helper" />
      {tunLabel && <span className="label text-ink">{tunLabel}</span>}
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
            {windows ? (
              <li>Windows shows a User Account Control prompt. The helper is installed as a Windows service.</li>
            ) : (
              <>
                <li>macOS asks for your administrator password (“{APP_NAME} wants to make changes”).</li>
                <li>macOS may show a “Background Items Added” notification — that is the helper.</li>
              </>
            )}
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
          <HelperError error={error} />
        ))}
    </div>
  );
}

/** Install / uninstall failure: message, then the helper's own detail as selectable text (owner: copy into test reports). */
function HelperError({ error }: { error: AppError }) {
  if (!error.detail) return <ErrorNote error={error} />;
  return (
    <Note tone="error" title={error.message}>
      <p className="label mt-1 text-ink-muted">Details · {error.code}</p>
      <pre className="mt-1 cursor-text font-mono text-[11px] leading-[16px] break-all whitespace-pre-wrap text-ink-muted select-text">{error.detail}</pre>
    </Note>
  );
}
