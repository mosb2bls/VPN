import type { Protocol } from "../api/bindings";

export function rate(bytesPerSec: number | null | undefined): string {
  if (bytesPerSec == null) return "—";
  const units = ["B/s", "KB/s", "MB/s", "GB/s"];
  let v = bytesPerSec;
  let i = 0;
  while (v >= 1000 && i < units.length - 1) {
    v /= 1000;
    i++;
  }
  return `${v >= 100 || i === 0 ? Math.round(v) : v.toFixed(1)} ${units[i]}`;
}

export function bytes(n: number | null | undefined): string {
  return n == null ? "—" : rate(n).replace("/s", "");
}

/** hh:mm:ss since an RFC 3339 timestamp. */
export function since(iso: string | null | undefined, now = Date.now()): string {
  if (!iso) return "00:00:00";
  const s = Math.max(0, Math.floor((now - Date.parse(iso)) / 1000));
  const p = (n: number) => String(n).padStart(2, "0");
  return `${p(Math.floor(s / 3600))}:${p(Math.floor((s % 3600) / 60))}:${p(s % 60)}`;
}

export const protocolBadge = (p: Protocol): string => (p === "hysteria2" ? "HY2" : "REALITY");
export const transport = (p: Protocol): string => (p === "hysteria2" ? "UDP" : "TCP");
