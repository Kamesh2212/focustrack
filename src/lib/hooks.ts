import { useCallback, useEffect, useRef, useState } from "react";

interface Loaded<T> {
  data: T | null;
  error: string | null;
  loading: boolean;
  reload: () => void;
}

/**
 * Runs `fn` whenever `deps` change and, if `intervalMs` is set, polls while
 * the window is visible. Keeps showing the previous data during reloads.
 */
export function useLoad<T>(fn: () => Promise<T>, deps: unknown[], intervalMs?: number): Loaded<T> {
  const [data, setData] = useState<T | null>(null);
  const [error, setError] = useState<string | null>(null);
  const [loading, setLoading] = useState(true);
  const fnRef = useRef(fn);
  fnRef.current = fn;

  const run = useCallback(() => {
    let cancelled = false;
    setLoading(true);
    fnRef
      .current()
      .then((d) => {
        if (!cancelled) {
          setData(d);
          setError(null);
        }
      })
      .catch((e) => !cancelled && setError(String(e)))
      .finally(() => !cancelled && setLoading(false));
    return () => {
      cancelled = true;
    };
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, deps);

  useEffect(() => run(), [run]);

  useEffect(() => {
    if (!intervalMs) return;
    const id = window.setInterval(() => {
      if (document.visibilityState === "visible") run();
    }, intervalMs);
    const onVisible = () => document.visibilityState === "visible" && run();
    document.addEventListener("visibilitychange", onVisible);
    return () => {
      window.clearInterval(id);
      document.removeEventListener("visibilitychange", onVisible);
    };
  }, [run, intervalMs]);

  return { data, error, loading, reload: run };
}

/** Re-renders every `ms` so "live" durations tick. */
export function useNow(ms = 1000): number {
  const [now, setNow] = useState(Date.now());
  useEffect(() => {
    const id = window.setInterval(() => setNow(Date.now()), ms);
    return () => window.clearInterval(id);
  }, [ms]);
  return now;
}
