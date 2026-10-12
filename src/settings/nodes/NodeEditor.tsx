// Manual add / edit form for a node (SPEC §2.1, §2.1.1, §7). Secrets are never
// shown: on edit each secret shows its masked value and stays `keep` until the
// user chooses Change (→ `set`) or, for the optional obfs password, Remove
// (→ `clear`). Certificate pin and spiderX are not supported by the pinned core
// and are hidden — always sent as null (SPEC 1.0.1 / 1.2.1).
import { useState, type ReactNode } from "react";
import { commands } from "../../api/client";
import type { AppError, Hy2Form, Node, NodeForm, Protocol, SecretInput, VlessForm } from "../../api/bindings";
import { Segmented } from "../../ui/Segmented";
import { Button, Checkbox, ErrorNote, Field, Note, TextInput, fieldErrorFor } from "../kit";

const FINGERPRINTS = ["chrome", "firefox", "safari", "edge", "ios", "android", "360", "qq", "random", "randomized"];

type SecretState = { mode: "keep" } | { mode: "set"; value: string } | { mode: "clear" };
const toInput = (s: SecretState): SecretInput => (s.mode === "set" ? { kind: "set", value: s.value } : { kind: s.mode });

interface Draft {
  protocol: Protocol;
  name: string;
  server: string;
  port: string;
  // HY2
  serverPorts: string;
  password: SecretState;
  sni: string;
  insecure: boolean;
  obfsPassword: SecretState;
  upMbps: string;
  downMbps: string;
  // REALITY
  uuid: SecretState;
  realitySni: string;
  fingerprint: string;
  publicKey: SecretState;
  shortId: SecretState;
}

function draftFrom(node: Node | null): Draft {
  const add = !node;
  const blank: SecretState = add ? { mode: "set", value: "" } : { mode: "keep" };
  const d: Draft = {
    protocol: node?.protocol ?? "hysteria2",
    name: node?.name ?? "",
    server: node?.server ?? "",
    port: node ? String(node.port) : "",
    serverPorts: "",
    password: blank,
    sni: "",
    insecure: false,
    obfsPassword: { mode: "keep" },
    upMbps: "",
    downMbps: "",
    uuid: blank,
    realitySni: "",
    fingerprint: "chrome",
    publicKey: blank,
    shortId: blank,
  };
  if (node?.details.protocol === "hysteria2") {
    const h = node.details;
    Object.assign(d, {
      serverPorts: h.serverPorts.join(", "),
      sni: h.sni ?? "",
      insecure: h.insecure,
      upMbps: h.upMbps?.toString() ?? "",
      downMbps: h.downMbps?.toString() ?? "",
    });
  } else if (node?.details.protocol === "vless_reality") {
    Object.assign(d, { realitySni: node.details.sni, fingerprint: node.details.fingerprint });
  }
  return d;
}

const optNum = (s: string) => (s.trim() === "" ? null : Number(s));

function toForm(d: Draft): NodeForm {
  const common = { name: d.name.trim(), server: d.server.trim(), port: Number(d.port) };
  if (d.protocol === "hysteria2") {
    const f: Hy2Form = {
      ...common,
      serverPorts: d.serverPorts
        .split(",")
        .map((x) => x.trim())
        .filter(Boolean),
      password: toInput(d.password),
      sni: d.sni.trim() || null,
      insecure: d.insecure,
      obfsPassword: toInput(d.obfsPassword),
      pinSha256: null, // not supported yet (SPEC §2.1.1)
      upMbps: optNum(d.upMbps),
      downMbps: optNum(d.downMbps),
    };
    return { protocol: "hysteria2", ...f };
  }
  const f: VlessForm = {
    ...common,
    uuid: toInput(d.uuid),
    sni: d.realitySni.trim(),
    fingerprint: d.fingerprint,
    publicKey: toInput(d.publicKey),
    shortId: toInput(d.shortId),
    spiderX: null, // not supported by the pinned core (SPEC 1.2.1)
  };
  return { protocol: "vless_reality", ...f };
}

export function NodeEditor({
  node,
  isActive,
  onDone,
  onCancel,
}: {
  /** null = add. */
  node: Node | null;
  /** Node currently carrying traffic — edits apply after reconnecting (SPEC §9.16). */
  isActive: boolean;
  onDone: (saved: Node, note: "reconnect" | null) => void;
  onCancel: () => void;
}) {
  const [d, setD] = useState<Draft>(() => draftFrom(node));
  const [error, setError] = useState<AppError | null>(null);
  const [busy, setBusy] = useState(false);
  const set = <K extends keyof Draft>(k: K, v: Draft[K]) => setD((x) => ({ ...x, [k]: v }));
  const fe = (f: string) => fieldErrorFor(error, f);
  const add = !node;

  const save = async () => {
    setBusy(true);
    setError(null);
    const form = toForm(d);
    const r = add ? await commands.addNodeManual(form) : await commands.updateNode(node.id, form);
    setBusy(false);
    if (r.status === "error") return setError(r.error);
    onDone(r.data, isActive ? "reconnect" : null);
  };

  const masked = (field: "password" | "obfsPassword" | "uuid" | "publicKey" | "shortId") => {
    if (!node) return null;
    const det = node.details as unknown as Record<string, string | null>;
    return det[`${field}Masked`] ?? null;
  };

  return (
    <form
      className="flex flex-col gap-4 border border-line bg-paper-raised p-4"
      onSubmit={(e) => {
        e.preventDefault();
        void save();
      }}
      aria-label={add ? "Add node manually" : `Edit ${node.name}`}
    >
      <div className="flex items-center justify-between">
        <span className="label">{add ? "Add node" : `Edit · ${node.name}`}</span>
        {add ? (
          <div className="w-[180px]">
            <Segmented<Protocol>
              label="Protocol"
              options={[
                { value: "hysteria2", label: "HY2" },
                { value: "vless_reality", label: "REALITY" },
              ]}
              value={d.protocol}
              onChange={(p) => set("protocol", p)}
            />
          </div>
        ) : (
          <span className="label border border-ink px-1">{d.protocol === "hysteria2" ? "HY2" : "REALITY"}</span>
        )}
      </div>

      {node?.secretMissing && (
        <Note tone="warn" title="Credentials missing — re-enter them">
          The saved credentials for this node are no longer in the system keychain. Enter them again below.
        </Note>
      )}

      <Field label="Name" error={fe("name")} htmlFor="n-name">
        <TextInput id="n-name" value={d.name} onChange={(e) => set("name", e.target.value)} invalid={!!fe("name")} maxLength={64} placeholder="London HY2" />
      </Field>

      <div className="grid grid-cols-[1fr_96px] gap-3">
        <Field label="Server" error={fe("server")} htmlFor="n-server" help="Domain name or IPv4 address (IPv6 is not supported).">
          <TextInput id="n-server" mono value={d.server} onChange={(e) => set("server", e.target.value)} invalid={!!fe("server")} placeholder="edge.example.net" />
        </Field>
        <Field label="Port" error={fe("port")} htmlFor="n-port">
          <TextInput id="n-port" mono inputMode="numeric" value={d.port} onChange={(e) => set("port", e.target.value)} invalid={!!fe("port")} placeholder="443" />
        </Field>
      </div>

      {d.protocol === "hysteria2" ? (
        <>
          <SecretField label="Password" field="password" state={d.password} onChange={(s) => set("password", s)} masked={masked("password")} error={fe("password")} add={add} />
          <Field label="Port hopping" optional error={fe("serverPorts")} htmlFor="n-hop" help="Extra port ranges, e.g. 20000-30000. Leave empty for a single port.">
            <TextInput id="n-hop" mono value={d.serverPorts} onChange={(e) => set("serverPorts", e.target.value)} invalid={!!fe("serverPorts")} placeholder="20000-30000" />
          </Field>
          <Field label="Server name (SNI)" optional error={fe("sni")} htmlFor="n-sni" help="Defaults to the server name.">
            <TextInput id="n-sni" mono value={d.sni} onChange={(e) => set("sni", e.target.value)} invalid={!!fe("sni")} />
          </Field>
          <SecretField
            label="Obfuscation password (Salamander)"
            field="obfsPassword"
            optional
            state={d.obfsPassword}
            onChange={(s) => set("obfsPassword", s)}
            masked={masked("obfsPassword")}
            error={fe("obfsPassword")}
            add={add}
            removable
          />
          <div className="grid grid-cols-2 gap-3">
            <Field label="Upload Mbps" optional error={fe("upMbps")} htmlFor="n-up">
              <TextInput id="n-up" mono inputMode="numeric" value={d.upMbps} onChange={(e) => set("upMbps", e.target.value)} invalid={!!fe("upMbps")} />
            </Field>
            <Field label="Download Mbps" optional error={fe("downMbps")} htmlFor="n-down">
              <TextInput id="n-down" mono inputMode="numeric" value={d.downMbps} onChange={(e) => set("downMbps", e.target.value)} invalid={!!fe("downMbps")} />
            </Field>
          </div>
          <Checkbox
            checked={d.insecure}
            onChange={(v) => set("insecure", v)}
            label="Skip certificate verification (insecure)"
            help="Only for testing. Anyone between you and the server could read or change your traffic."
          />
          {d.insecure && <Note tone="warn" title="Certificate verification is off for this node" />}
        </>
      ) : (
        <>
          <SecretField label="UUID" field="uuid" state={d.uuid} onChange={(s) => set("uuid", s)} masked={masked("uuid")} error={fe("uuid")} add={add} mono />
          <Field label="Server name (SNI)" error={fe("sni")} htmlFor="n-rsni" help="The camouflage site the REALITY server presents.">
            <TextInput id="n-rsni" mono value={d.realitySni} onChange={(e) => set("realitySni", e.target.value)} invalid={!!fe("sni")} placeholder="www.example.org" />
          </Field>
          <SecretField label="Public key" field="publicKey" state={d.publicKey} onChange={(s) => set("publicKey", s)} masked={masked("publicKey")} error={fe("publicKey")} add={add} mono />
          <SecretField
            label="Short ID"
            field="shortId"
            state={d.shortId}
            onChange={(s) => set("shortId", s)}
            masked={masked("shortId")}
            error={fe("shortId")}
            add={add}
            mono
            help="0–16 hex characters; may be empty."
          />
          <Field label="TLS fingerprint" error={fe("fingerprint")} htmlFor="n-fp">
            <select
              id="n-fp"
              value={d.fingerprint}
              onChange={(e) => set("fingerprint", e.target.value)}
              className="h-8 border border-line bg-paper-raised px-2 font-mono text-[12px] focus-visible:outline-2 focus-visible:outline-cobalt"
            >
              {FINGERPRINTS.map((f) => (
                <option key={f}>{f}</option>
              ))}
            </select>
          </Field>
          <p className="text-secondary text-ink-muted">Flow xtls-rprx-vision over TCP is fixed.</p>
        </>
      )}

      {error && error.code !== "VALIDATION_FAILED" && <ErrorNote error={error} />}
      {error?.code === "VALIDATION_FAILED" && <Note tone="error" title={error.message} />}

      <div className="flex items-center justify-between gap-3">
        <p className="text-secondary text-ink-muted">Secrets are stored in the system keychain. macOS may ask to allow access — choose Always Allow.</p>
        <div className="flex shrink-0 gap-2">
          <Button variant="ghost" onClick={onCancel}>
            Cancel
          </Button>
          <Button variant="solid" type="submit" busy={busy}>
            {busy ? "Saving…" : add ? "Add node" : "Save"}
          </Button>
        </div>
      </div>
    </form>
  );
}

function SecretField({
  label,
  field,
  state,
  onChange,
  masked,
  error,
  add,
  optional,
  removable,
  mono,
  help,
}: {
  label: string;
  field: string;
  state: SecretState;
  onChange: (s: SecretState) => void;
  masked: string | null;
  error?: ReturnType<typeof fieldErrorFor>;
  add: boolean;
  optional?: boolean;
  removable?: boolean;
  mono?: boolean;
  help?: ReactNode;
}) {
  const id = `n-${field}`;
  const editing = state.mode === "set";
  // Optional secret on add: empty means "not set".
  if (add || editing)
    return (
      <Field label={label} optional={optional} error={error} htmlFor={id} help={help}>
        <div className="flex gap-2">
          <TextInput
            id={id}
            type="password"
            autoComplete="off"
            mono={mono}
            value={state.mode === "set" ? state.value : ""}
            onChange={(e) => onChange(e.target.value === "" && optional && add ? { mode: "keep" } : { mode: "set", value: e.target.value })}
            invalid={!!error}
          />
          {!add && (
            <Button variant="ghost" onClick={() => onChange({ mode: "keep" })}>
              Keep current
            </Button>
          )}
        </div>
      </Field>
    );
  return (
    <Field label={label} optional={optional} error={error} help={help}>
      <div className="flex h-8 items-center gap-3">
        <span className="font-mono text-[12px] text-ink-muted">
          {state.mode === "clear" ? "Will be removed" : masked === null ? "Not set" : masked === "" ? "(empty)" : masked}
        </span>
        <Button variant="ghost" onClick={() => onChange({ mode: "set", value: "" })}>
          {masked === null ? "Set" : "Change"}
        </Button>
        {removable && masked !== null && state.mode !== "clear" && (
          <Button variant="ghost" onClick={() => onChange({ mode: "clear" })}>
            Remove
          </Button>
        )}
      </div>
    </Field>
  );
}
