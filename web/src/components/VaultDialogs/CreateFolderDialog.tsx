import { useEffect, useMemo, useState } from 'react';
import { Close, FolderOpen } from '../icons/Icon';
import { TreeNode } from '../../api/client';
import { collectFolders } from '../../lib/tree';
import { useAsyncDialog } from '../../lib/useAsyncDialog';
import styles from './dialog.module.css';

interface Props {
  open: boolean;
  tree: TreeNode[];
  onClose: () => void;
  /** Returns the path the new folder was created at. Caller throws on
   *  collision so we can surface the error inline. */
  onCreate: (parent: string, name: string) => Promise<void>;
}

/** Pick a parent + name, then create the folder. Mirrors MoveDialog's
 *  parent-picker so the two flows feel like the same surface. */
export function CreateFolderDialog({ open, tree, onClose, onCreate }: Props) {
  const [parent, setParent] = useState<string>('');
  const [name, setName] = useState('');
  const { busy, error, run } = useAsyncDialog(open, 'create failed');

  useEffect(() => {
    if (open) {
      setParent('');
      setName('');
    }
  }, [open]);

  const folders = useMemo(() => collectFolders(tree), [tree]);

  const trimmedName = name.trim();
  const canSubmit = !busy && trimmedName.length > 0;
  const preview = trimmedName
    ? parent
      ? `${parent}/${trimmedName}`
      : trimmedName
    : null;

  async function submit() {
    if (!canSubmit) return;
    await run(() => onCreate(parent, trimmedName), { onSuccess: onClose });
  }

  if (!open) return null;

  return (
    <div className={styles.scrim} onMouseDown={onClose}>
      <div className={styles.panel} onMouseDown={(e) => e.stopPropagation()}>
        <div className={styles.header}>
          <div>
            <h2 className={styles.title}>New folder</h2>
            <div className={styles.subtitle}>
              Choose where it lives, then give it a name.
            </div>
          </div>
          <button type="button" className={styles.close} onClick={onClose} aria-label="Close">
            <Close size={14} />
          </button>
        </div>

        <div>
          <label className={styles.label}>Parent</label>
          <div className={styles.folderList}>
            {folders.map((f) => (
              <button
                key={f.path || 'root'}
                type="button"
                className={`${styles.folderItem} ${
                  parent === f.path ? styles.folderItemActive : ''
                }`}
                style={{ paddingLeft: 10 + f.depth * 16 }}
                onClick={() => setParent(f.path)}
                disabled={busy}
              >
                <FolderOpen size={12} /> {f.label}
              </button>
            ))}
          </div>
        </div>

        <div>
          <label className={styles.label}>Name</label>
          <div className={styles.field}>
            <input
              autoFocus
              className={styles.fieldInput}
              value={name}
              onChange={(e) => setName(e.target.value)}
              placeholder="e.g. Recipes"
              disabled={busy}
              onKeyDown={(e) => {
                if (e.key === 'Enter') {
                  e.preventDefault();
                  void submit();
                }
              }}
            />
          </div>
          {preview && (
            <div className={styles.subtitle} style={{ marginTop: 6 }}>
              Will be created at <code>{preview}</code>
            </div>
          )}
        </div>

        {error && <div className={styles.error} role="alert">{error}</div>}

        <div className={styles.actions}>
          <button type="button" className={styles.cancelBtn} onClick={onClose}>
            Cancel
          </button>
          <button
            type="button"
            className={styles.submitBtn}
            onClick={submit}
            disabled={!canSubmit}
          >
            {busy ? 'Creating…' : 'Create folder'}
          </button>
        </div>
      </div>
    </div>
  );
}
