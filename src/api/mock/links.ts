// Simplified share-link parser for the mock, following SPEC §2.1.1. Enough to
// exercise every import outcome in the UI; the real parser lives in crates/core.
// Error details name the parameter, never its value.
import type { AppError, ErrorCode, Node } from "../bindings";
import { appError } from "./errors";
import { mask, newId } from "./fixtures";

export type ParsedLink =
  | { ok: true; node: Node; credential: string }
  | { ok: false; scheme: string | null; error: AppError };

const fail = (scheme: string | null, code: ErrorCode, detail: string): ParsedLink => ({
  ok: false,
  scheme,
  error: appError(code, { detail }),
});

const HY2_PARAMS = new Set(["sni", "insecure", "obfs", "obfs-password", "mport", "upmbps", "downmbps", "ech"]);
const VLESS_PARAMS = new Set(["encryption", "security", "sni", "fp", "pbk", "sid", "spx", "type", "flow", "headerType", "alpn", "pqv"]);
const param = (k: string) => `Parameter: ${k}.`;
const FINGERPRINTS = new Set(["chrome", "firefox", "safari", "ios", "android", "edge", "360", "qq", "random", "randomized"]);

export function parseLink(line: string): ParsedLink {
  const m = /^([a-z0-9+.-]+):\/\//i.exec(line);
  const scheme = m ? m[1].toLowerCase() : null;
  if (!scheme || !["hy2", "hysteria2", "vless"].includes(scheme)) {
    return fail(scheme, "LINK_UNSUPPORTED_SCHEME", `scheme: ${scheme ?? "none"}`);
  }
  let url: URL;
  try {
    url = new URL(line);
  } catch {
    return fail(scheme, "LINK_MALFORMED", "not a valid URL");
  }
  let cred: string;
  try {
    cred = decodeURIComponent(url.username);
  } catch {
    return fail(scheme, "LINK_MALFORMED", "Invalid percent-encoding.");
  }
  const server = url.hostname;
  const q = url.searchParams;
  if (!server) return fail(scheme, "LINK_MALFORMED", "Missing host.");
  if (server.startsWith("[")) return fail(scheme, "LINK_INVALID_VALUE", "Server: IPv6 addresses are not supported (IPv4-only).");
  if (!cred) return fail(scheme, "LINK_MISSING_PARAM", "Missing credential.");
  const keys = [...q.keys()];
  const dup = keys.find((k, i) => keys.indexOf(k) !== i);
  if (dup) return fail(scheme, "LINK_MALFORMED", param(dup));
  if (scheme === "vless" && !url.port) return fail(scheme, "LINK_MISSING_PARAM", "Port.");
  const port = Number(url.port || 443);
  const rawName = decodeURIComponent(url.hash.replace(/^#/, "")).replace(/\s+/g, " ").trim().slice(0, 64);
  const name = rawName || `${scheme === "vless" ? "REALITY" : "HY2"} ${server}`;

  if (scheme === "vless") {
    for (const k of keys) if (!VLESS_PARAMS.has(k)) return fail(scheme, "LINK_UNSUPPORTED_PARAM", param(k));
    if (q.get("pqv")) return fail(scheme, "LINK_UNSUPPORTED_PARAM", param("pqv"));
    if (q.get("security") !== "reality") return fail(scheme, q.has("security") ? "LINK_UNSUPPORTED_PARAM" : "LINK_MISSING_PARAM", param("security"));
    if (!["tcp", "raw"].includes(q.get("type") ?? "tcp")) return fail(scheme, "LINK_UNSUPPORTED_PARAM", param("type"));
    if ((q.get("headerType") ?? "none") !== "none") return fail(scheme, "LINK_UNSUPPORTED_PARAM", param("headerType"));
    if ((q.get("encryption") ?? "none") !== "none") return fail(scheme, "LINK_UNSUPPORTED_PARAM", param("encryption"));
    if (q.get("flow") !== "xtls-rprx-vision") return fail(scheme, q.has("flow") ? "LINK_UNSUPPORTED_PARAM" : "LINK_MISSING_PARAM", param("flow"));
    if (!q.get("pbk")) return fail(scheme, "LINK_MISSING_PARAM", param("pbk"));
    const isIp = /^\d+\.\d+\.\d+\.\d+$/.test(server);
    const sni = q.get("sni") || (isIp ? null : server);
    if (!sni) return fail(scheme, "LINK_MISSING_PARAM", param("sni"));
    if (!/^[0-9a-f]{8}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{12}$/i.test(cred)) return fail(scheme, "LINK_INVALID_VALUE", "UUID.");
    const fp = q.get("fp") ?? "chrome";
    if (!FINGERPRINTS.has(fp)) return fail(scheme, "LINK_INVALID_VALUE", param("fp"));
    if (!/^([0-9a-f]{2}){0,8}$/i.test(q.get("sid") ?? "")) return fail(scheme, "LINK_INVALID_VALUE", param("sid"));
    return {
      ok: true,
      credential: cred,
      node: {
        id: newId(),
        name,
        protocol: "vless_reality",
        server,
        port,
        details: {
          protocol: "vless_reality",
          uuidMasked: mask(cred),
          flow: "xtls-rprx-vision",
          sni,
          fingerprint: fp,
          publicKeyMasked: mask(q.get("pbk")!),
          shortIdMasked: q.get("sid") ? "••••" : "",
          spiderX: q.get("spx"),
        },
        lastTest: null,
        secretMissing: false,
      },
    };
  }

  for (const k of keys) if (!HY2_PARAMS.has(k)) return fail(scheme, "LINK_UNSUPPORTED_PARAM", param(k));
  if (q.get("ech")) return fail(scheme, "LINK_UNSUPPORTED_PARAM", param("ech"));
  const insecure = q.get("insecure") ?? "0";
  if (!["0", "1", "true", "false"].includes(insecure)) return fail(scheme, "LINK_INVALID_VALUE", param("insecure"));
  const obfs = q.get("obfs");
  if (obfs && obfs !== "salamander") return fail(scheme, "LINK_UNSUPPORTED_PARAM", param("obfs"));
  if (!!obfs !== !!q.get("obfs-password")) return fail(scheme, "LINK_MISSING_PARAM", param(obfs ? "obfs-password" : "obfs"));
  const mport = q.get("mport");
  if (mport && !/^\d+(-\d+)?(,\d+(-\d+)?)*$/.test(mport)) return fail(scheme, "LINK_INVALID_VALUE", param("mport"));
  for (const k of ["upmbps", "downmbps"]) {
    const v = q.get(k);
    if (v !== null && !(Number(v) >= 1 && Number(v) <= 100_000)) return fail(scheme, "LINK_INVALID_VALUE", param(k));
  }
  const num = (k: string) => (q.get(k) ? Number(q.get(k)) : null);
  return {
    ok: true,
    credential: cred,
    node: {
      id: newId(),
      name,
      protocol: "hysteria2",
      server,
      port,
      details: {
        protocol: "hysteria2",
        passwordMasked: mask(cred),
        serverPorts: mport ? mport.split(",") : [],
        sni: q.get("sni"),
        insecure: insecure === "1" || insecure === "true",
        obfsPasswordMasked: obfs ? mask(q.get("obfs-password")!) : null,
        pinSha256: null,
        upMbps: num("upmbps"),
        downMbps: num("downmbps"),
      },
      lastTest: null,
      secretMissing: false,
    },
  };
}

/** Example links for the dev panel. Fake hosts/credentials only. */
export const SAMPLE_LINKS = [
  "hy2://mock-pass-abcdef@hy2.example.net:24443?sni=hy2.example.net#Imported HY2",
  "vless://00000000-aaaa-4bbb-8ccc-0000000000ff@reality.example.net:443?encryption=none&security=reality&sni=www.example.org&fp=chrome&pbk=mockPbkBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBB&sid=ab12&type=tcp&flow=xtls-rprx-vision#Imported REALITY",
  "ss://not-supported@example.com:8388#Shadowsocks",
  "hy2://mock-pass-xyz@hy2.example.net:24443?sni=hy2.example.net&pinSHA256=AB12#Pinned (unsupported)",
  "hy2://mock-pass-v6@[2001:db8::1]:24443#IPv6 server",
].join("\n");
