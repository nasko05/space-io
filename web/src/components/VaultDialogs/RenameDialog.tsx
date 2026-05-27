import { FormEvent, useEffect, useState } from 'react';
import { Close } from '../icons/Icon';
import styles from './dialog.module.css';

interface Props {
  open: boolean;
  /** Original visible name (file: with extension; folder: just the leaf). */
  currentName: string;
  /** Other names already present in the same parent folder, lowercased. */
  siblingNames: Set<string>;
  onClose: () => void;
  onRename: (newName: string) => Promise<void>;
}

export function RenameDialog({ open, currentName, siblingNames, onClose, onRename }: Props) {
  const [name, setName] = useState(currentName);
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState<string | null>(null);

  useEffect(() => {
    if (open) {
      setName(currentName);
      setBusy(false);
      setError(null);
    }
  }, [open, currentName]);

  if (!open) return null;

  function validate(value: string): string | null {
    const trimmed = value.trim();
    if (!trimmed) return 'Name cannot be empty.';
    if (trimmed.includes('/') || trimmed.includes('\\')) return 'Slashes are not allowed.';
    if (trimmed.startsWith('.')) return 'Names cannot start with a dot.';
    if (trimmed === currentName) return null; // no-op
    if (siblingNames.has(trimmed.toLowerCase())) return 'Something here already has that name.';
    return null;
  }

  async function submit(e: FormEvent) {
    e.preventDefault();
    const err = validate(name);
    if (err) {
      setError(err);
      return;
    }
    if (name.trim() === currentName) {
      onClose();
      return;
    }
    setBusy(true);
    setError(null);
    try {
      await onRename(name.trim());
      onClose();
    } catch (e) {
      setError(e instanceof Error ? e.message : 'rename failed');
      setBusy(false);
    }
  }

  return (
    <div className={styles.scrim} onMouseDown={onClose}>
      <form className={styles.panel} onMouseDown={(e) => e.stopPropagation()} onSubmit={submit}>
        <div className={styles.header}>
          <div>
            <h2 className={styles.title}>Rename</h2>
            <div className={styles.subtitle}>Was: {currentName}</div>
          </div>
          <button type="button" className={styles.close} onClick={onClose} aria-label="Close">
            <Close size={14} />
          </button>
        </div>

        <div>
          <label className={styles.label}>New name</label>
          <div className={styles.field}>
            <input
              className={styles.fieldInput}
              value={name}
              onChange={(e) => {
                setName(e.target.value);
                setError(null);
              }}
              autoFocus
              spellCheck={false}
            />
          </div>
          {error && <div className={styles.error} role="alert">{error}</div>}
        </div>

        <div className={styles.actions}>
          <button type="button" className={styles.cancelBtn} onClick={onClose}>
            Cancel
          </button>
          <button type="submit" className={styles.submitBtn} disabled={busy || !name.trim()}>
            {busy ? 'Renaming…' : 'Rename'}
          </button>
        </div>
      </form>
    </div>
  );
}
