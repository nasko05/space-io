import { useState } from 'react';
import { Close } from '../icons/Icon';
import styles from './dialog.module.css';

interface Props {
  open: boolean;
  count: number;
  /** Used in the body sentence for single-item deletes. */
  sampleName?: string;
  onClose: () => void;
  onConfirm: () => Promise<void>;
}

export function DeleteConfirmDialog({
  open,
  count,
  sampleName,
  onClose,
  onConfirm,
}: Props) {
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState<string | null>(null);

  if (!open) return null;

  async function go() {
    setBusy(true);
    setError(null);
    try {
      await onConfirm();
      onClose();
    } catch (e) {
      setError(e instanceof Error ? e.message : 'delete failed');
      setBusy(false);
    }
  }

  return (
    <div className={styles.scrim} onMouseDown={onClose}>
      <div className={styles.panel} onMouseDown={(e) => e.stopPropagation()}>
        <div className={styles.header}>
          <div>
            <h2 className={styles.title}>Delete?</h2>
            <div className={styles.subtitle}>
              Soft delete — recoverable from <em>.trash/</em> in the space.
            </div>
          </div>
          <button type="button" className={styles.close} onClick={onClose} aria-label="Close">
            <Close size={14} />
          </button>
        </div>

        <div className={styles.body}>
          {count === 1 && sampleName ? (
            <>
              Move <em>{sampleName}</em> to the trash.
            </>
          ) : (
            <>
              Move <em>{count} items</em> to the trash.
            </>
          )}
          <br />
          Tags follow the file; the git history is preserved.
        </div>

        {error && <div className={styles.error} role="alert">{error}</div>}

        <div className={styles.actions}>
          <button type="button" className={styles.cancelBtn} onClick={onClose}>
            Cancel
          </button>
          <button type="button" className={styles.destructiveBtn} onClick={go} disabled={busy}>
            {busy ? 'Deleting…' : 'Move to trash'}
          </button>
        </div>
      </div>
    </div>
  );
}
