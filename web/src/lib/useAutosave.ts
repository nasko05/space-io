import { useCallback, useEffect, useRef, useState } from 'react';

export type SaveStatus =
  | { kind: 'idle' }
  | { kind: 'dirty' }
  | { kind: 'saving' }
  | { kind: 'saved'; at: number }
  | { kind: 'error'; message: string };

interface Options {
  delayMs?: number;
  onSave: (value: string) => Promise<void>;
}

/**
 * Debounced autosave. Returns:
 *   - `status` — the live save state
 *   - `markDirty(next)` — call when the user edits; schedules a save
 *   - `flush()` — immediately persists any pending change (e.g. on Cmd+S or
 *     before navigating away)
 *
 * The hook is reset whenever the `onSave` identity changes (i.e. when the
 * caller swaps to a different file path).
 */
export function useAutosave({ delayMs = 800, onSave }: Options) {
  const [status, setStatus] = useState<SaveStatus>({ kind: 'idle' });
  const pendingRef = useRef<string | null>(null);
  const timerRef = useRef<number | null>(null);
  const inFlightRef = useRef(false);
  const onSaveRef = useRef(onSave);

  useEffect(() => {
    // Flush any edit still pending for the previous target before repointing,
    // or switching files within the debounce window would drop the last edits.
    const previousSave = onSaveRef.current;
    const pending = pendingRef.current;
    if (pending !== null) {
      void previousSave(pending);
    }
    onSaveRef.current = onSave;
    pendingRef.current = null;
    if (timerRef.current !== null) {
      clearTimeout(timerRef.current);
      timerRef.current = null;
    }
    setStatus({ kind: 'idle' });
  }, [onSave]);

  const runSave = useCallback(async () => {
    if (inFlightRef.current) return;
    const value = pendingRef.current;
    if (value === null) return;
    pendingRef.current = null;
    inFlightRef.current = true;
    setStatus({ kind: 'saving' });
    try {
      await onSaveRef.current(value);
      // If more edits arrived while we were saving, schedule another round.
      if (pendingRef.current !== null) {
        inFlightRef.current = false;
        setStatus({ kind: 'dirty' });
        scheduleRef.current();
      } else {
        setStatus({ kind: 'saved', at: Date.now() });
        inFlightRef.current = false;
      }
    } catch (err) {
      inFlightRef.current = false;
      setStatus({
        kind: 'error',
        message: err instanceof Error ? err.message : 'save failed',
      });
    }
  }, []);

  const scheduleRef = useRef<() => void>(() => {});
  scheduleRef.current = () => {
    if (timerRef.current !== null) clearTimeout(timerRef.current);
    timerRef.current = window.setTimeout(() => {
      timerRef.current = null;
      void runSave();
    }, delayMs);
  };

  const markDirty = useCallback(
    (next: string) => {
      pendingRef.current = next;
      setStatus({ kind: 'dirty' });
      scheduleRef.current();
    },
    [],
  );

  const flush = useCallback(async () => {
    if (timerRef.current !== null) {
      clearTimeout(timerRef.current);
      timerRef.current = null;
    }
    await runSave();
  }, [runSave]);

  useEffect(
    () => () => {
      if (timerRef.current !== null) clearTimeout(timerRef.current);
      // Don't lose a pending edit if the editor unmounts mid-debounce.
      if (pendingRef.current !== null) {
        void onSaveRef.current(pendingRef.current);
        pendingRef.current = null;
      }
    },
    [],
  );

  // Best-effort flush when the tab/window is closing. Browsers won't await an
  // async handler here, so this is a last-ditch attempt, not a guarantee.
  useEffect(() => {
    const flushOnUnload = () => {
      if (pendingRef.current !== null) {
        void onSaveRef.current(pendingRef.current);
      }
    };
    window.addEventListener('beforeunload', flushOnUnload);
    return () => window.removeEventListener('beforeunload', flushOnUnload);
  }, []);

  return { status, markDirty, flush };
}

export function saveStatusLabel(status: SaveStatus): string {
  switch (status.kind) {
    case 'idle':
      return '';
    case 'dirty':
      return 'unsaved';
    case 'saving':
      return 'saving…';
    case 'saved': {
      const secs = Math.max(1, Math.round((Date.now() - status.at) / 1000));
      return `auto-saved ${secs}s ago`;
    }
    case 'error':
      return `save failed: ${status.message}`;
  }
}
