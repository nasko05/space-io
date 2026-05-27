import { useEffect, useState } from 'react';
import { api, HistoryEntry } from '../../api/client';
import { Branch, Close, Commit } from '../icons/Icon';
import styles from './HistoryPanel.module.css';

interface Props {
  open: boolean;
  path: string | null;
  onClose: () => void;
}

export function HistoryPanel({ open, path, onClose }: Props) {
  const [entries, setEntries] = useState<HistoryEntry[]>([]);
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState<string | null>(null);

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
  }, [open, path]);

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
            <em>No commits yet.</em>
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
