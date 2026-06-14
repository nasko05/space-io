import { useCallback, useEffect, useState } from 'react';

/**
 * Shared state machine for the vault dialogs (Rename / Move / Tags / Delete).
 *
 * Every dialog has the same shape:
 *   - reset `busy` and `error` when the dialog opens
 *   - while an action is in flight, show "Working…"
 *   - on failure, surface the message but keep the dialog open
 *   - on success, let the caller close it
 *
 * Returns a `run` that wraps an async action: it flips `busy`, awaits, and
 * either calls `onSuccess` (typically `onClose`) or stashes the error.
 */
export function useAsyncDialog(open: boolean, fallbackMessage = 'Action failed') {
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState<string | null>(null);

  useEffect(() => {
    if (open) {
      setBusy(false);
      setError(null);
    }
  }, [open]);

  const clearError = useCallback(() => setError(null), []);

  const run = useCallback(
    async (
      action: () => Promise<void>,
      options?: { onSuccess?: () => void },
    ): Promise<void> => {
      setBusy(true);
      setError(null);
      try {
        await action();
      } catch (err) {
        setError(err instanceof Error ? err.message : fallbackMessage);
        return;
      } finally {
        setBusy(false);
      }
      options?.onSuccess?.();
    },
    [fallbackMessage],
  );

  return { busy, error, run, clearError, setError } as const;
}
