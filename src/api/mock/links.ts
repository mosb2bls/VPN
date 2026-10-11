// Simplified share-link parser for the mock. Enough to exercise every import
// outcome in the UI; the real parser lives in crates/core.
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

const HY2_PARAMS = new Set(["sni", "insecure", "obfs", "obfs-password", "pinSHA256", "mport", "upmbps", "downmbps"]);
const VLESS_PARAMS = new Set(["encryption", "security", "sni", "fp", "pbk", "sid", "spx", "type", "flow", "headerType"]);
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
  const cred = decodeURIComponent(url.username);
  const server = url.hostname;
  const port = Number(url.port || 443);
  const name = decodeURIComponent(url.hash.replace(/^#/, "")) || `${server}:${port}`;
  const q = url.searchParams;
  if (!server) return fail(scheme, "LINK_MALFORMED", "missing host");
  if (!cred) return fail(scheme, "LINK_MISSING_PARAM", "missing credential");

  if (scheme === "vless") {
    for (const k of q.keys()) if (!VLESS_PARAMS.has(k)) return fail(scheme, "LINK_UNSUPPORTED_PARAM", `parameter: ${k}`);
    if (q.get("security") !== "reality") return fail(scheme, "LINK_UNSUPPORTED_PARAM", "security must be reality");
    if ((q.get("type") ?? "tcp") !== "tcp") return fail(scheme, "LINK_UNSUPPORTED_PARAM", "type must be tcp");
    if (q.get("flow") !== "xtls-rprx-vision") return fail(scheme, "LINK_UNSUPPORTED_PARAM", "flow must be xtls-rprx-vision");
    for (const k of ["pbk", "sni"]) if (!q.get(k)) return fail(scheme, "LINK_MISSING_PARAM", `parameter: ${k}`);
    if (!/^[0-9a-f-]{36}$/i.test(cred)) return fail(scheme, "LINK_INVALID_VALUE", "uuid");
    const fp = q.get("fp") ?? "chrome";
    if (!FINGERPRINTS.has(fp)) return fail(scheme, "LINK_INVALID_VALUE", "fp");
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
          sni: q.get("sni")!,
          fingerprint: fp,
          publicKeyMasked: mask(q.get("pbk")!),
          shortIdMasked: mask(q.get("sid") ?? ""),
          spiderX: q.get("spx"),
        },
        lastTest: null,
      },
    };
  }

  for (const k of q.keys()) if (!HY2_PARAMS.has(k)) return fail(scheme, "LINK_UNSUPPORTED_PARAM", `parameter: ${k}`);
  const obfs = q.get("obfs");
  if (obfs && obfs !== "salamander") return fail(scheme, "LINK_UNSUPPORTED_PARAM", "obfs must be salamander");
  if (obfs && !q.get("obfs-password")) return fail(scheme, "LINK_MISSING_PARAM", "parameter: obfs-password");
  const mport = q.get("mport");
  if (mport && !/^\d+(-\d+)?(,\d+(-\d+)?)*$/.test(mport)) return fail(scheme, "LINK_INVALID_VALUE", "mport");
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
        insecure: q.get("insecure") === "1",
        obfsPasswordMasked: obfs ? mask(q.get("obfs-password")!) : null,
        pinSha256: q.get("pinSHA256"),
        upMbps: num("upmbps"),
        downMbps: num("downmbps"),
      },
      lastTest: null,
    },
  };
}

/** Example links for the dev panel. Fake hosts/credentials only. */
export const SAMPLE_LINKS = [
  "hy2://mock-pass-abcdef@hy2.example.net:24443?sni=hy2.example.net#Imported HY2",
  "vless://00000000-aaaa-4bbb-8ccc-0000000000ff@reality.example.net:443?encryption=none&security=reality&sni=www.example.org&fp=chrome&pbk=mockPbkBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBB&sid=ab12&type=tcp&flow=xtls-rprx-vision#Imported REALITY",
  "ss://not-supported@example.com:8388#Shadowsocks",
  "hy2://mock-pass-xyz@hy2.example.net:24443?sni=hy2.example.net&alpn=h3#Has unsupported param",
].join("\n");
