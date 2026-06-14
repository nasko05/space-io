import { DialogShell } from './DialogShell';
import { useAsyncDialog } from '../../lib/useAsyncDialog';
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
  const { busy, error, run } = useAsyncDialog(open, 'delete failed');

  if (!open) { return null; }

  async function confirmDelete() {
    await run(onConfirm, { onSuccess: onClose });
  }

  return (
    <DialogShell
      title="Delete?"
      subtitle={<>Soft delete — recoverable from <em>.trash/</em> in the space.</>}
      onClose={onClose}
    >
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
          <button type="button" className={styles.destructiveBtn} onClick={confirmDelete} disabled={busy}>
            {busy ? 'Deleting…' : 'Move to trash'}
          </button>
        </div>
    </DialogShell>
  );
}
