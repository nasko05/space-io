import { useEffect, useState } from 'react';
import { api, HistoryEntry } from '../../api/client';
import { Branch, Close, Commit } from '../icons/Icon';
import styles from './HistoryPanel.module.css';

interface Props {
  open: boolean;
  path: string | null;
  /** Bumped by the parent after a new checkpoint so the list reloads to show
   *  it without the user reopening the panel. */
  reloadToken?: number;
  onClose: () => void;
  /** Called after a successful rollback so the parent can reload the file
   *  and refresh the tree. Without this the rail / Today list would still
   *  show the pre-rollback excerpt until the next manual refresh. */
  onRollback?: (path: string, commit: string) => Promise<void>;
}

export function HistoryPanel({ open, path, reloadToken, onClose, onRollback }: Props) {
  const [entries, setEntries] = useState<HistoryEntry[]>([]);
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const [restoring, setRestoring] = useState<string | null>(null);

  async function restore(commit: string) {
    if (!path || !onRollback || restoring) return;
    if (
      !window.confirm(
        `Restore this file to ${commit.slice(0, 7)}? A new checkpoint is added on top — nothing is lost.`,
      )
    ) {
      return;
    }
    setRestoring(commit);
    setError(null);
    try {
      await onRollback(path, commit);
      // Refresh the history list so the new "Rollback …" commit shows up
      // at the top.
      const { entries } = await api.history(path);
      setEntries(entries);
    } catch (err) {
      setError(err instanceof Error ? err.message : 'rollback failed');
    } finally {
      setRestoring(null);
    }
  }

  useEffect(() => {
    if (!open || !path) return;
    let cancelled = false;
    setBusy(true);
    setError(null);
    api
      .history(path)
      .then(({ entries }) => {
        if (!cancelled) setEntries(entries);
      })
      .catch((err) => {
        if (!cancelled) {
          setError(err instanceof Error ? err.message : 'failed to load history');
          setEntries([]);
        }
      })
      .finally(() => {
        if (!cancelled) setBusy(false);
      });
    return () => {
      cancelled = true;
    };
  }, [open, path, reloadToken]);

  if (!open) return null;

  return (
    <aside className={styles.panel}>
      <div className={styles.header}>
        <span className={styles.title}>
          <Branch size={13} /> History
        </span>
        <span className={styles.path}>{path}</span>
        <button type="button" className={styles.close} onClick={onClose} aria-label="Close">
          <Close size={14} />
        </button>
      </div>
      <div className={styles.body}>
        {busy && <div className={styles.empty}>Loading…</div>}
        {error && <div className={styles.error}>{error}</div>}
        {!busy && !error && entries.length === 0 && (
          <div className={styles.empty}>
            <em>No checkpoints yet.</em>
          </div>
        )}
        <ol className={styles.list}>
          {entries.map((e, i) => (
            <li key={e.commit} className={styles.entry}>
              <span className={styles.dot}>
                <Commit size={12} />
              </span>
              <div className={styles.entryBody}>
                <div className={styles.entryMessage}>{e.message || '(no message)'}</div>
                <div className={styles.entryMeta}>
                  <span className={styles.entryAuthor}>{e.author}</span>
                  <span>·</span>
                  <span>{formatWhen(e.when)}</span>
                  <span>·</span>
                  <span className={styles.entryHash}>{e.commit.slice(0, 7)}</span>
                  {i === 0 && <span className={styles.entryHead}>HEAD</span>}
                </div>
              </div>
              {i !== 0 && onRollback && (
                <button
                  type="button"
                  className={styles.restoreBtn}
                  onClick={() => void restore(e.commit)}
                  disabled={restoring !== null}
                  title="Restore the file to this version"
                >
                  {restoring === e.commit ? 'Restoring…' : 'Restore'}
                </button>
              )}
            </li>
          ))}
        </ol>
      </div>
    </aside>
  );
}

function formatWhen(iso: string): string {
  if (!iso) return '';
  const d = new Date(iso);
  if (isNaN(d.getTime())) return iso;
  const now = new Date();
  const diff = (now.getTime() - d.getTime()) / 1000;
  if (diff < 60) return 'just now';
  if (diff < 3600) return `${Math.round(diff / 60)} min ago`;
  if (diff < 86400) return `${Math.round(diff / 3600)}h ago`;
  if (diff < 86400 * 7) return `${Math.round(diff / 86400)}d ago`;
  return d.toLocaleString(undefined, {
    month: 'short',
    day: 'numeric',
    hour: '2-digit',
    minute: '2-digit',
    hour12: false,
  });
}
