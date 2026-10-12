// Import panels for the Nodes section: paste links, import result, QR image.
// Rules shown as help text come from SPEC §2.1.1. Error details name the
// parameter only — never values (the backend guarantees this).
import { useEffect, useRef, useState } from "react";
import { getCurrentWebview } from "@tauri-apps/api/webview";
import { IN_TAURI, USE_MOCK, commands } from "../../api/client";
import type { AppError, ImportResult, QrImageSource } from "../../api/bindings";
import { Button, ErrorNote, Note } from "../kit";

export const LINK_RULES =
  "One link per line: hy2:// or hysteria2://, and vless:// with REALITY + Vision over TCP. Not supported: certificate pin (pinSHA256), spiderX (spx), post-quantum REALITY (pqv), other transports, and IPv6 server addresses.";

export function PasteLinks({ onResult, onCancel }: { onResult: (r: ImportResult) => void; onCancel: () => void }) {
  const [text, setText] = useState("");
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState<AppError | null>(null);
  const ref = useRef<HTMLTextAreaElement>(null);
  useEffect(() => ref.current?.focus(), []);
  const go = async () => {
    setBusy(true);
    setError(null);
    const r = await commands.importLinks(text);
    setBusy(false);
    if (r.status === "error") setError(r.error);
    else {
      onResult(r.data);
      if (r.data.errors.length === 0) setText("");
    }
  };
  return (
    <div className="flex flex-col gap-3 border border-line bg-paper-raised p-4">
      <span className="label">Paste links</span>
      <textarea
        ref={ref}
        rows={4}
        value={text}
        onChange={(e) => setText(e.target.value)}
        spellCheck={false}
        placeholder={"hy2://…\nvless://…"}
        aria-label="Share links, one per line"
        className="w-full resize-y border border-line bg-paper px-2 py-1.5 font-mono text-[12px] leading-[18px] outline-none focus:border-ink focus-visible:outline-2 focus-visible:outline-cobalt"
      />
      <p className="text-secondary text-ink-muted">{LINK_RULES}</p>
      {error && <ErrorNote error={error} />}
      <div className="flex justify-end gap-2">
        <Button variant="ghost" onClick={onCancel}>
          Cancel
        </Button>
        <Button variant="solid" busy={busy} disabled={!text.trim()} onClick={() => void go()}>
          {busy ? "Importing…" : "Import"}
        </Button>
      </div>
    </div>
  );
}

/** Outcome of any import: added nodes + every rejected line with its reason. */
export function ImportResultNote({ result, onClose }: { result: ImportResult; onClose: () => void }) {
  const { added, errors } = result;
  return (
    <div className="flex flex-col gap-2">
      <Note
        tone={errors.length && !added.length ? "error" : errors.length ? "warn" : "ok"}
        title={`${added.length} node${added.length === 1 ? "" : "s"} added${errors.length ? ` · ${errors.length} rejected` : ""}`}
      >
        {added.length > 0 && <p>{added.map((n) => n.name).join(", ")}</p>}
        {errors.length > 0 && (
          <ul className="mt-1 flex flex-col gap-1">
            {errors.map((e) => (
              <li key={`${e.line}-${e.error.code}`} className="flex gap-2">
                <span className="label w-[92px] shrink-0 pt-px text-ink-muted">
                  Line {e.line}
                  {e.scheme ? ` · ${e.scheme}` : ""}
                </span>
                <span>
                  {e.error.message}
                  {e.error.detail && <span className="text-ink-muted"> {e.error.detail}</span>}
                </span>
              </li>
            ))}
          </ul>
        )}
      </Note>
      <div>
        <Button variant="ghost" onClick={onClose}>
          Dismiss
        </Button>
      </div>
    </div>
  );
}

const toBase64 = async (blob: Blob) => {
  const bytes = new Uint8Array(await blob.arrayBuffer());
  let s = "";
  for (let i = 0; i < bytes.length; i += 0x8000) s += String.fromCharCode(...bytes.subarray(i, i + 0x8000));
  return btoa(s);
};

/**
 * QR image: drop a PNG/JPEG (Tauri gives a file path — `{kind:"path"}`), choose a
 * file or paste an image (bytes — `{kind:"bytes"}`). Decoding can take a few
 * seconds, so the panel shows a busy state (SPEC 1.1.1).
 */
export function QrImport({ onResult, onCancel }: { onResult: (r: ImportResult) => void; onCancel: () => void }) {
  const [busy, setBusy] = useState(false);
  const [over, setOver] = useState(false);
  const [error, setError] = useState<AppError | null>(null);
  const fileRef = useRef<HTMLInputElement>(null);

  const run = async (source: QrImageSource) => {
    setBusy(true);
    setError(null);
    const r = await commands.importQrImage(source);
    setBusy(false);
    if (r.status === "error") setError(r.error);
    else onResult(r.data);
  };
  const fromBlob = async (b: Blob) => {
    if (!/^image\/(png|jpe?g)$/.test(b.type)) {
      setError({ code: "QR_IMAGE_UNREADABLE", message: "The image could not be opened. Use a PNG or JPEG file.", recoverable: true, fields: [], detail: null, restoreReport: null });
      return;
    }
    await run({ kind: "bytes", base64: await toBase64(b) });
  };

  // Paste an image (Cmd/Ctrl+V) while the panel is open.
  useEffect(() => {
    const onPaste = (e: ClipboardEvent) => {
      const item = [...(e.clipboardData?.items ?? [])].find((i) => i.type.startsWith("image/"));
      const f = item?.getAsFile();
      if (f) {
        e.preventDefault();
        void fromBlob(f);
      }
    };
    window.addEventListener("paste", onPaste);
    return () => window.removeEventListener("paste", onPaste);
  });

  // Inside Tauri the settings window receives native drag-drop with file paths.
  useEffect(() => {
    if (!IN_TAURI || USE_MOCK) return;
    let off: (() => void) | undefined;
    void getCurrentWebview()
      .onDragDropEvent((ev) => {
        const p = ev.payload;
        if (p.type === "over" || p.type === "enter") setOver(true);
        else if (p.type === "leave") setOver(false);
        else if (p.type === "drop") {
          setOver(false);
          if (p.paths[0]) void run({ kind: "path", path: p.paths[0] });
        }
      })
      .then((u) => (off = u));
    return () => off?.();
  }, []);

  return (
    <div className="flex flex-col gap-3 border border-line bg-paper-raised p-4">
      <span className="label">QR image</span>
      <div
        onDragOver={(e) => {
          e.preventDefault();
          setOver(true);
        }}
        onDragLeave={() => setOver(false)}
        onDrop={(e) => {
          e.preventDefault();
          setOver(false);
          const f = e.dataTransfer.files[0];
          if (f) void fromBlob(f);
        }}
        className={`grid h-28 place-items-center border border-dashed text-center transition-colors duration-fast ${
          over ? "border-ink bg-ink/5" : "border-line"
        }`}
        aria-busy={busy || undefined}
      >
        {busy ? (
          <span className="label text-ink">Decoding QR code…</span>
        ) : (
          <div className="flex flex-col items-center gap-2">
            <span className="text-body">Drop a PNG or JPEG here, or paste an image</span>
            <Button onClick={() => fileRef.current?.click()}>Choose image…</Button>
            <input
              ref={fileRef}
              type="file"
              accept="image/png,image/jpeg"
              className="hidden"
              onChange={(e) => {
                const f = e.target.files?.[0];
                e.target.value = "";
                if (f) void fromBlob(f);
              }}
            />
          </div>
        )}
      </div>
      <p className="text-secondary text-ink-muted">One QR code per image, up to 15 MB. The image is decoded on this computer and never uploaded.</p>
      {error && <ErrorNote error={error} />}
      <div className="flex justify-end">
        <Button variant="ghost" onClick={onCancel}>
          Close
        </Button>
      </div>
    </div>
  );
}
