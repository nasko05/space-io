import { useEffect, useMemo, useState } from 'react';
import { Close, FolderOpen, Plus } from '../icons/Icon';
import { TreeFolder, TreeNode } from '../../api/client';
import styles from './dialog.module.css';

interface Props {
  open: boolean;
  tree: TreeNode[];
  /** Paths that are being moved — used to disable target folders that would
   * create a loop (moving a folder into itself or a descendant). */
  movingPaths: string[];
  onClose: () => void;
  onMove: (destinationFolder: string) => Promise<void>;
  onCreateFolder: (parent: string, name: string) => Promise<string>;
}

interface FolderEntry {
  path: string; // '' for root
  label: string;
  depth: number;
}

function collectFolders(tree: TreeNode[]): FolderEntry[] {
  const out: FolderEntry[] = [{ path: '', label: '/ (space root)', depth: 0 }];
  const walk = (nodes: TreeNode[], depth: number) => {
    const folders = nodes.filter((n): n is TreeFolder => n.type === 'folder');
    folders.sort((a, b) => a.name.localeCompare(b.name));
    for (const f of folders) {
      out.push({ path: f.path, label: f.name, depth });
      walk(f.children, depth + 1);
    }
  };
  walk(tree, 1);
  return out;
}

export function MoveDialog({
  open,
  tree,
  movingPaths,
  onClose,
  onMove,
  onCreateFolder,
}: Props) {
  const [picked, setPicked] = useState<string>('');
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const [creating, setCreating] = useState(false);
  const [newFolderName, setNewFolderName] = useState('');

  useEffect(() => {
    if (open) {
      setPicked('');
      setBusy(false);
      setError(null);
      setCreating(false);
      setNewFolderName('');
    }
  }, [open]);

  const folders = useMemo(() => collectFolders(tree), [tree]);

  function isLoopTarget(folder: string): boolean {
    return movingPaths.some(
      (p) => folder === p || folder.startsWith(`${p}/`),
    );
  }

  async function submit() {
    if (busy) return;
    setBusy(true);
    setError(null);
    try {
      await onMove(picked);
      onClose();
    } catch (e) {
      setError(e instanceof Error ? e.message : 'move failed');
      setBusy(false);
    }
  }

  async function createFolderInside() {
    const name = newFolderName.trim();
    if (!name) return;
    setBusy(true);
    setError(null);
    try {
      const created = await onCreateFolder(picked, name);
      setPicked(created);
      setCreating(false);
      setNewFolderName('');
    } catch (e) {
      setError(e instanceof Error ? e.message : 'could not create folder');
    } finally {
      setBusy(false);
    }
  }

  if (!open) return null;

  return (
    <div className={styles.scrim} onMouseDown={onClose}>
      <div className={styles.panel} onMouseDown={(e) => e.stopPropagation()}>
        <div className={styles.header}>
          <div>
            <h2 className={styles.title}>
              Move {movingPaths.length === 1 ? '1 item' : `${movingPaths.length} items`}
            </h2>
            <div className={styles.subtitle}>Choose a destination folder.</div>
          </div>
          <button type="button" className={styles.close} onClick={onClose} aria-label="Close">
            <Close size={14} />
          </button>
        </div>

        <div className={styles.folderList}>
          {folders.map((f) => {
            const disabled = isLoopTarget(f.path);
            return (
              <button
                key={f.path || 'root'}
                type="button"
                className={`${styles.folderItem} ${
                  picked === f.path ? styles.folderItemActive : ''
                }`}
                style={{ paddingLeft: 10 + f.depth * 16 }}
                onClick={() => setPicked(f.path)}
                disabled={disabled || busy}
                title={disabled ? "Can't move into itself" : undefined}
              >
                <FolderOpen size={12} /> {f.label}
              </button>
            );
          })}

          {creating ? (
            <div className={styles.folderItem} style={{ paddingLeft: 10 }}>
              <Plus size={12} />
              <input
                className={styles.chipInput}
                value={newFolderName}
                onChange={(e) => setNewFolderName(e.target.value)}
                placeholder="New folder name…"
                autoFocus
                onKeyDown={(e) => {
                  if (e.key === 'Enter') {
                    e.preventDefault();
                    void createFolderInside();
                  } else if (e.key === 'Escape') {
                    setCreating(false);
                    setNewFolderName('');
                  }
                }}
              />
            </div>
          ) : (
            <button
              type="button"
              className={`${styles.folderItem} ${styles.folderItemNew}`}
              onClick={() => setCreating(true)}
              disabled={busy}
            >
              <Plus size={12} /> New folder in {picked ? `'${labelFor(picked)}'` : '/'}
            </button>
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
            disabled={busy || isLoopTarget(picked)}
          >
            {busy ? 'Moving…' : `Move here`}
          </button>
        </div>
      </div>
    </div>
  );
}

function labelFor(path: string): string {
  if (!path) return '/';
  return path.split('/').pop() ?? path;
}
