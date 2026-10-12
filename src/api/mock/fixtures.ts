// Fake data for the mock backend. All hosts use reserved example domains /
// TEST-NET addresses; all secrets are made up. Never put real values here.
import type {
  AppSettings,
  FailoverSettings,
  HelperStatus,
  Node,
  RulesStatus,
  SafetySettings,
} from "../bindings";

export const mask = (v: string): string => (v.length < 8 ? "••••" : `${v.slice(0, 4)}…••••`);

export interface NodeFixture {
  node: Node;
  /** Fake credential (password / UUID) — used only for duplicate detection. */
  credential: string;
}

let seq = 0;
export function newId(): string {
  seq += 1;
  const hex = (n: number, len: number) => n.toString(16).padStart(len, "0").slice(-len);
  return `00000000-0000-4000-8000-${hex(Date.now(), 8)}${hex(seq, 4)}`;
}

const hy2 = (
  id: string,
  name: string,
  server: string,
  port: number,
  password: string,
  extra: Partial<Extract<Node["details"], { protocol: "hysteria2" }>> = {},
  lastTest: Node["lastTest"] = null,
): NodeFixture => ({
  credential: password,
  node: {
    id,
    name,
    protocol: "hysteria2",
    server,
    port,
    details: {
      protocol: "hysteria2",
      passwordMasked: mask(password),
      serverPorts: [],
      sni: server,
      insecure: false,
      obfsPasswordMasked: null,
      pinSha256: null,
      upMbps: null,
      downMbps: null,
      ...extra,
    },
    lastTest,
    secretMissing: false,
  },
});

const vless = (
  id: string,
  name: string,
  server: string,
  port: number,
  uuid: string,
  extra: Partial<Extract<Node["details"], { protocol: "vless_reality" }>> = {},
  lastTest: Node["lastTest"] = null,
): NodeFixture => ({
  credential: uuid,
  node: {
    id,
    name,
    protocol: "vless_reality",
    server,
    port,
    details: {
      protocol: "vless_reality",
      uuidMasked: mask(uuid),
      flow: "xtls-rprx-vision",
      sni: "www.example.org",
      fingerprint: "chrome",
      publicKeyMasked: mask("mockPublicKeyAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAA"),
      shortIdMasked: "••••", // SPEC 1.1.0: always "••••" (or "" when empty)
      spiderX: null,
      ...extra,
    },
    lastTest,
    secretMissing: false,
  },
});

const tested = (nodeId: string, latencyMs: number, minutesAgo: number): Node["lastTest"] => ({
  nodeId,
  ok: true,
  latencyMs,
  exitCountry: "GB",
  error: null,
  testedAt: new Date(Date.now() - minutesAgo * 60_000).toISOString().replace(/\.\d+Z$/, "Z"),
});

export const ID_HY2 = "11111111-1111-4111-8111-111111111111";
export const ID_REALITY = "22222222-2222-4222-8222-222222222222";
export const ID_HOP = "33333333-3333-4333-8333-333333333333";

export function defaultNodes(): NodeFixture[] {
  return [
    hy2(ID_HY2, "London HY2", "edge.example.net", 24443, "mock-hy2-password-1", {}, tested(ID_HY2, 48, 12)),
    vless(
      ID_REALITY,
      "London REALITY",
      "edge.example.net",
      443,
      "00000000-aaaa-4bbb-8ccc-000000000001",
      {},
      tested(ID_REALITY, 112, 12),
    ),
    hy2(ID_HOP, "Test HY2 · port hopping", "203.0.113.10", 443, "mock-hy2-password-2", {
      serverPorts: ["20000-30000"],
      insecure: true,
      sni: null,
      obfsPasswordMasked: mask("mock-obfs-password"),
      upMbps: 50,
      downMbps: 200,
    }),
  ];
}

export function manyNodes(): NodeFixture[] {
  const base = defaultNodes();
  const extra: NodeFixture[] = [
    hy2(newId(), "Backup HY2 — secondary VPS with a deliberately very long display name", "backup.example.net", 24443, "mock-hy2-password-3"),
    vless(newId(), "Backup REALITY", "backup.example.net", 443, "00000000-aaaa-4bbb-8ccc-000000000002"),
    hy2(newId(), "Office", "198.51.100.7", 8443, "mock-hy2-password-4"),
    vless(newId(), "REALITY · alt fingerprint", "edge.example.net", 8443, "00000000-aaaa-4bbb-8ccc-000000000003", {
      fingerprint: "safari",

    }),
    hy2(newId(), "Travel", "travel.example.com", 443, "mock-hy2-password-5"),
  ];
  // "Office" lost its keychain entry (SPEC §9.15).
  const office = extra.find((f) => f.node.name === "Office");
  if (office) office.node = { ...office.node, secretMissing: true };
  return [...base, ...extra];
}

export function oneNode(): NodeFixture[] {
  return defaultNodes().slice(0, 1);
}

export const DEFAULT_FAILOVER: FailoverSettings = {
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

export const DEFAULT_SAFETY: SafetySettings = {
  killSwitch: true,
  directFallback: false,
  allowLan: true,
};

export const DEFAULT_SETTINGS: AppSettings = {
  launchAtLogin: false,
  autoConnect: false,
  closeToTray: true,
  rulesAutoUpdate: true,
  rulesUpdateIntervalHours: 24,
  logLevel: "info",
};

export function defaultRules(): RulesStatus {
  const day = 86_400_000;
  const iso = (t: number) => new Date(t).toISOString().replace(/\.\d+Z$/, "Z");
  return {
    version: "2026.10.10", // as the real bundle (SPEC 1.3.0)
    source: "bundled",
    updatedAt: "2026-10-10T06:12:00Z",
    lastCheckAt: iso(Date.now() - 3 * 3_600_000),
    nextCheckAt: iso(Date.now() + day - 3 * 3_600_000),
    updating: false,
    sets: [
      { name: "geosite-cn", sizeBytes: 1_284_902, sha256Prefix: "b07e55d1a2c4" },
      { name: "geoip-cn", sizeBytes: 412_388, sha256Prefix: "3f9a12c48e07" },
    ],
    lastError: null,
  };
}

export function defaultHelper(): HelperStatus {
  return { installed: false, running: false, version: null, expectedVersion: "0.1.0", needsUpgrade: false };
}
