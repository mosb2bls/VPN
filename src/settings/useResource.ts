// Load a settings object from the backend and save it back, keeping the last
// error (incl. VALIDATION_FAILED field errors). Used by the settings sections.
import { useCallback, useEffect, useState } from "react";
import type { AppError } from "../api/bindings";
import type { Result } from "../api/types";

export function useResource<T>(get: () => Promise<Result<T>>, put: (v: T) => Promise<Result<T>>) {
  const [saved, setSaved] = useState<T | null>(null);
  const [draft, setDraft] = useState<T | null>(null);
  const [error, setError] = useState<AppError | null>(null);
  const [busy, setBusy] = useState(false);

  const load = useCallback(async () => {
    const r = await get();
    if (r.status === "ok") {
      setSaved(r.data);
      setDraft(r.data);
    } else setError(r.error);
  }, [get]);
  useEffect(() => void load(), [load]);

  /** Save `next` (or the draft). Returns true on success. */
  const save = useCallback(
    async (next?: T) => {
      const v = next ?? draft;
      if (!v) return false;
      setBusy(true);
      setError(null);
      const r = await put(v);
      setBusy(false);
      if (r.status === "ok") {
        setSaved(r.data);
        setDraft(r.data);
        return true;
      }
      setError(r.error);
      if (next) setDraft(saved); // immediate saves revert on failure
      return false;
    },
    [draft, put, saved],
  );

  const dirty = JSON.stringify(saved) !== JSON.stringify(draft);
  return { saved, draft, setDraft, save, busy, error, setError, dirty, reload: load };
}
