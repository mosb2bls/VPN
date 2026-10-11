// Validation for the mock, following the rules in SPEC §2.3, §2.6 and §7.
import type { AppSettings, FailoverSettings, FieldError, Node, NodeForm, SecretInput } from "../bindings";
import { fieldError } from "./errors";

const HOST = /^(?=.{1,253}$)([a-z0-9]([a-z0-9-]{0,61}[a-z0-9])?)(\.[a-z0-9]([a-z0-9-]{0,61}[a-z0-9])?)*$/i;
const IPV4 = /^((25[0-5]|2[0-4]\d|1?\d?\d)\.){3}(25[0-5]|2[0-4]\d|1?\d?\d)$/;
const UUID = /^[0-9a-f]{8}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{12}$/i;
export const FINGERPRINTS = ["chrome", "firefox", "safari", "ios", "android", "edge", "360", "qq", "random", "randomized"];

const validHost = (h: string) => IPV4.test(h) || HOST.test(h);

function secret(
  out: FieldError[],
  field: string,
  s: SecretInput,
  mode: "add" | "update",
  required: boolean,
  check?: (v: string) => string | null,
) {
  if (s.kind === "clear") {
    if (required) out.push(fieldError(field, "FIELD_CANNOT_CLEAR"));
    return;
  }
  if (s.kind === "keep") {
    if (mode === "add" && required) out.push(fieldError(field, "FIELD_REQUIRED"));
    return;
  }
  if (s.value === "" && field !== "shortId") {
    out.push(fieldError(field, "FIELD_REQUIRED"));
    return;
  }
  const code = check?.(s.value);
  if (code) out.push(fieldError(field, code as FieldError["code"]));
}

export function validateNodeForm(form: NodeForm, mode: "add" | "update"): FieldError[] {
  const out: FieldError[] = [];
  if (!form.name.trim()) out.push(fieldError("name", "FIELD_REQUIRED"));
  else if (form.name.length > 64) out.push(fieldError("name", "FIELD_TOO_LONG"));
  if (!form.server.trim()) out.push(fieldError("server", "FIELD_REQUIRED"));
  else if (form.server.includes(":")) out.push(fieldError("server", "FIELD_INVALID_HOST")); // IPv6 rejected
  else if (!validHost(form.server.trim())) out.push(fieldError("server", "FIELD_INVALID_HOST"));
  if (!Number.isInteger(form.port) || form.port < 1 || form.port > 65535) out.push(fieldError("port", "FIELD_INVALID_PORT"));

  if (form.protocol === "hysteria2") {
    for (const r of form.serverPorts) {
      const m = /^(\d+)(?:-(\d+))?$/.exec(r.trim());
      const a = m ? Number(m[1]) : 0;
      const b = m?.[2] ? Number(m[2]) : a;
      if (!m || a < 1 || b > 65535 || a > b) {
        out.push(fieldError("serverPorts", "FIELD_INVALID_PORT_RANGE"));
        break;
      }
    }
    secret(out, "password", form.password, mode, true);
    secret(out, "obfsPassword", form.obfsPassword, mode, false);
    if (form.sni && !validHost(form.sni)) out.push(fieldError("sni", "FIELD_INVALID_SNI"));
    // SPEC §2.1.1: pinning not supported yet — must be null.
    if (form.pinSha256 !== null) out.push(fieldError("pinSha256", "FIELD_INVALID"));
    for (const f of ["upMbps", "downMbps"] as const) {
      const v = form[f];
      if (v !== null && (!Number.isFinite(v) || v < 1 || v > 100_000)) out.push(fieldError(f, "FIELD_INVALID_BANDWIDTH"));
    }
  } else {
    secret(out, "uuid", form.uuid, mode, true, (v) => (UUID.test(v) ? null : "FIELD_INVALID_UUID"));
    secret(out, "publicKey", form.publicKey, mode, true, (v) =>
      /^[A-Za-z0-9_-]{43}=?$/.test(v) ? null : "FIELD_INVALID_PUBLIC_KEY",
    );
    secret(out, "shortId", form.shortId, mode, true, (v) =>
      /^([0-9a-f]{2}){0,8}$/i.test(v) ? null : "FIELD_INVALID_SHORT_ID",
    );
    if (!form.sni.trim()) out.push(fieldError("sni", "FIELD_REQUIRED"));
    else if (!validHost(form.sni.trim())) out.push(fieldError("sni", "FIELD_INVALID_SNI"));
    if (!FINGERPRINTS.includes(form.fingerprint)) out.push(fieldError("fingerprint", "FIELD_INVALID_FINGERPRINT"));
    if (form.spiderX && !form.spiderX.startsWith("/")) out.push(fieldError("spiderX", "FIELD_INVALID"));
  }
  return out;
}

export function validateFailover(s: FailoverSettings, nodes: Node[]): FieldError[] {
  const out: FieldError[] = [];
  const range = (f: keyof FailoverSettings, v: number, lo: number, hi: number) => {
    if (!Number.isInteger(v) || v < lo || v > hi) out.push(fieldError(f, "FIELD_OUT_OF_RANGE"));
  };
  const hosts = new Set<string>();
  let badUrl = false;
  for (const t of s.probeTargets) {
    try {
      const u = new URL(t);
      if (u.protocol !== "http:" && u.protocol !== "https:") badUrl = true;
      hosts.add(u.hostname);
    } catch {
      badUrl = true;
    }
  }
  if (badUrl) out.push(fieldError("probeTargets", "FIELD_INVALID"));
  else if (s.probeTargets.length < 2 || s.probeTargets.length > 5 || hosts.size < 2)
    out.push(fieldError("probeTargets", "FIELD_OUT_OF_RANGE"));
  range("probeIntervalSecs", s.probeIntervalSecs, 5, 300);
  range("probeTimeoutSecs", s.probeTimeoutSecs, 1, 30);
  if (s.probeTimeoutSecs >= s.probeIntervalSecs && !out.some((e) => e.field === "probeTimeoutSecs"))
    out.push(fieldError("probeTimeoutSecs", "FIELD_OUT_OF_RANGE"));
  range("failureRounds", s.failureRounds, 1, 20);
  range("minDwellSecs", s.minDwellSecs, 0, 3600);
  range("returnStableSecs", s.returnStableSecs, 60, 86_400);
  range("maxSwitchesPerHour", s.maxSwitchesPerHour, 1, 60);
  if (s.fallbackNodeId !== null && !nodes.some((n) => n.id === s.fallbackNodeId))
    out.push(fieldError("fallbackNodeId", "NODE_NOT_FOUND"));
  return out;
}

export function validateSettings(s: AppSettings): FieldError[] {
  const v = s.rulesUpdateIntervalHours;
  return Number.isInteger(v) && v >= 1 && v <= 168 ? [] : [fieldError("rulesUpdateIntervalHours", "FIELD_OUT_OF_RANGE")];
}
