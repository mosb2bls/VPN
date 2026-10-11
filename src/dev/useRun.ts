// Dev-only: run a command, remember its outcome for display.
import { useCallback, useState } from "react";
import type { AppError } from "../api/bindings";
import type { Result } from "../api/types";
import { applyStatus } from "../state/store";

export interface RunOutcome {
  label: string;
  ok: boolean;
  error: AppError | null;
  summary: string | null;
}

const isStatus = (d: unknown): d is Parameters<typeof applyStatus>[0] =>
  !!d && typeof d === "object" && "state" in d && "captureMode" in d;

export function useRun() {
  const [last, setLast] = useState<RunOutcome | null>(null);
  const [busy, setBusy] = useState<string | null>(null);
  const run = useCallback(async <T,>(label: string, p: Promise<Result<T>>, summarize?: (d: T) => string) => {
    setBusy(label);
    const r = await p;
    setBusy(null);
    if (r.status === "ok") {
      if (isStatus(r.data)) applyStatus(r.data);
      setLast({ label, ok: true, error: null, summary: summarize ? summarize(r.data) : null });
    } else setLast({ label, ok: false, error: r.error, summary: null });
    return r;
  }, []);
  return { run, last, busy };
}
