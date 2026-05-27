import { DragEvent, useCallback, useEffect, useMemo, useState } from 'react';
import { WindowChrome } from '../WindowChrome/WindowChrome';
import { HearthRail } from '../Rail/HearthRail';
import { HearthCard } from './HearthCard';
import {
  Close,
  Download as DownloadIcon,
  FolderOpen,
  MoreHorizontal,
  Moon,
  Pencil,
  Plus,
  Sun,
  Tag,
} from '../icons/Icon';
import { ContextMenu, MenuItem } from '../ContextMenu/ContextMenu';
import { RenameDialog } from '../VaultDialogs/RenameDialog';
import { MoveDialog } from '../VaultDialogs/MoveDialog';
import { TagsDialog } from '../VaultDialogs/TagsDialog';
import { DeleteConfirmDialog } from '../VaultDialogs/DeleteConfirmDialog';
import { CreateFolderDialog } from '../VaultDialogs/CreateFolderDialog';
import { ExcerptMap, MetaMap, TreeFile, TreeFolder, TreeNode } from '../../api/client';
import { CalendarView, TodayEntry } from '../../lib/calendar';
import styles from './HearthVault.module.css';

const DRAG_MIME = 'application/x-hearth-path';

interface Props {
  tree: TreeNode[];
  excerpts: ExcerptMap;
  meta: MetaMap;
  calendar: CalendarView;
  entries: TodayEntry[];
  entriesLabel: string;
  selectedDay: number | null;
  onClearSelectedDay: () => void;
  onSelectFile: (path: string) => void;
  onSelectDay: (day: number) => void;
  onNewEntry: () => void;
  onBackToReader: () => void;
  onDownloadFile: (file: TreeFile) => void;
  onRenameFile: (from: string, to: string) => Promise<void>;
  onMoveFiles: (paths: string[], destinationFolder: string) => Promise<void>;
  onCreateFolder: (path: string) => Promise<void>;
  onDeleteFiles: (paths: string[]) => Promise<void>;
  onSetTags: (paths: string[], tags: string[]) => Promise<void>;
  onOpenPasskey?: () => void;
  hasPasskey?: boolean;
  theme?: 'light' | 'dark';
  onToggleTheme?: () => void;
}

type DialogState =
  | { kind: 'none' }
  | { kind: 'rename'; file: TreeFile }
  | { kind: 'rename-folder'; folder: TreeFolder }
  | { kind: 'move'; paths: string[] }
  | { kind: 'tags'; paths: string[] }
  | { kind: 'delete'; paths: string[] }
  | { kind: 'create-folder' };

interface MenuState {
  open: boolean;
  x: number;
  y: number;
  items: MenuItem[];
}

const EMPTY_MENU: MenuState = { open: false, x: 0, y: 0, items: [] };

export function HearthVault({
  tree,
  excerpts,
  meta,
  calendar,
  entries,
  entriesLabel,
  selectedDay,
  onClearSelectedDay,
  onSelectFile,
  onSelectDay,
  onNewEntry,
  onBackToReader,
  onDownloadFile,
  onRenameFile,
  onMoveFiles,
  onCreateFolder,
  onDeleteFiles,
  onSetTags,
  onOpenPasskey,
  hasPasskey,
  theme,
  onToggleTheme,
}: Props) {
  const folders: TreeFolder[] = tree.filter((n): n is TreeFolder => n.type === 'folder');
  const totalFiles = countFiles(tree);

  const [selection, setSelection] = useState<Set<string>>(new Set());
  const [anchor, setAnchor] = useState<string | null>(null);
  const [menu, setMenu] = useState<MenuState>(EMPTY_MENU);
  const [dialog, setDialog] = useState<DialogState>({ kind: 'none' });
  const [dropTarget, setDropTarget] = useState<string | null>(null);

  // Build an ordered list of all visible file paths so shift-select can
  // resolve a range using a stable index.
  const shelfFiles = useMemo(() => {
    const out: { folderPath: string; files: TreeFile[] }[] = [];
    for (const f of folders) {
      out.push({ folderPath: f.path, files: collectFilesUnder(f) });
    }
    return out;
  }, [folders]);

  const orderedPaths = useMemo(() => {
    const list: string[] = [];
    for (const shelf of shelfFiles) for (const f of shelf.files) list.push(f.path);
    return list;
  }, [shelfFiles]);

  // If files disappear (deleted / moved), drop them from the selection.
  useEffect(() => {
    setSelection((cur) => {
      const next = new Set<string>();
      for (const p of cur) if (orderedPaths.includes(p)) next.add(p);
      return next.size === cur.size ? cur : next;
    });
  }, [orderedPaths]);

  const knownTags = useMemo(() => {
    const set = new Set<string>();
    for (const v of Object.values(meta)) for (const t of v.tags) set.add(t);
    return Array.from(set).sort((a, b) => a.localeCompare(b));
  }, [meta]);

  const onToggleSelect = useCallback(
    (path: string, mods: { shift?: boolean; cmd?: boolean }) => {
      setSelection((cur) => {
        const next = new Set(cur);
        if (mods.shift && anchor) {
          const a = orderedPaths.indexOf(anchor);
          const b = orderedPaths.indexOf(path);
          if (a >= 0 && b >= 0) {
            const [lo, hi] = a < b ? [a, b] : [b, a];
            for (let i = lo; i <= hi; i += 1) next.add(orderedPaths[i]);
            return next;
          }
        }
        if (next.has(path)) next.delete(path);
        else next.add(path);
        return next;
      });
      if (!mods.shift) setAnchor(path);
    },
    [anchor, orderedPaths],
  );

  const clearSelection = useCallback(() => {
    setSelection(new Set());
    setAnchor(null);
  }, []);

  // ---- Context menu ----

  function fileContextItems(file: TreeFile): MenuItem[] {
    const selected = selection.has(file.path);
    const targets = selected && selection.size > 1 ? Array.from(selection) : [file.path];
    const multi = targets.length > 1;

    const items: MenuItem[] = [];
    if (!multi) {
      items.push({
        label: file.kind === 'md' ? 'Open' : 'Preview',
        icon: <FolderOpen size={13} />,
        onClick: () => onSelectFile(file.path),
      });
      items.push({
        label: 'Rename…',
        icon: <Pencil size={13} />,
        onClick: () => setDialog({ kind: 'rename', file }),
      });
    }
    items.push({
      label: multi ? `Move ${targets.length} items…` : 'Move to…',
      icon: <FolderOpen size={13} />,
      onClick: () => setDialog({ kind: 'move', paths: targets }),
    });
    items.push({
      label: multi ? `Edit tags on ${targets.length} items…` : 'Edit tags…',
      icon: <Tag size={13} />,
      onClick: () => setDialog({ kind: 'tags', paths: targets }),
    });
    if (!multi) {
      items.push({
        label: 'Save to disk',
        icon: <DownloadIcon size={13} />,
        onClick: () => onDownloadFile(file),
      });
    }
    items.push({ divider: true, label: '', onClick: () => {} });
    items.push({
      label: multi ? `Delete ${targets.length} items` : 'Delete',
      icon: <Close size={13} />,
      destructive: true,
      onClick: () => setDialog({ kind: 'delete', paths: targets }),
    });
    return items;
  }

  function openCardMenu(file: TreeFile, x: number, y: number) {
    // Right-click on a card that isn't part of the selection moves the
    // anchor onto it but doesn't change selection.
    setMenu({ open: true, x, y, items: fileContextItems(file) });
  }

  function folderMenuItems(folder: TreeFolder): MenuItem[] {
    return [
      {
        label: 'Rename folder…',
        icon: <Pencil size={13} />,
        onClick: () => setDialog({ kind: 'rename-folder', folder }),
      },
      {
        label: 'Move folder to…',
        icon: <FolderOpen size={13} />,
        onClick: () => setDialog({ kind: 'move', paths: [folder.path] }),
      },
      { divider: true, label: '', onClick: () => {} },
      {
        label: 'Delete folder',
        icon: <Close size={13} />,
        destructive: true,
        onClick: () => setDialog({ kind: 'delete', paths: [folder.path] }),
      },
    ];
  }

  function openFolderMenu(folder: TreeFolder, x: number, y: number) {
    setMenu({ open: true, x, y, items: folderMenuItems(folder) });
  }

  // ---- Drag-and-drop on shelves ----

  function onShelfDragOver(e: DragEvent<HTMLElement>, folderPath: string) {
    if (!e.dataTransfer.types.includes(DRAG_MIME)) return;
    e.preventDefault();
    e.dataTransfer.dropEffect = 'move';
    if (dropTarget !== folderPath) setDropTarget(folderPath);
  }

  function onShelfDragLeave(folderPath: string) {
    if (dropTarget === folderPath) setDropTarget(null);
  }

  function onShelfDrop(e: DragEvent<HTMLElement>, folderPath: string) {
    e.preventDefault();
    setDropTarget(null);
    const droppedPath = e.dataTransfer.getData(DRAG_MIME);
    if (!droppedPath) return;
    // If the dropped file is part of the current selection, move all
    // selected items; otherwise just the dropped one.
    const targets = selection.has(droppedPath) ? Array.from(selection) : [droppedPath];
    // Filter out files already in this folder (avoids no-op moves and
    // self-move attempts).
    const movable = targets.filter((p) => !p.startsWith(`${folderPath}/`));
    if (movable.length === 0) return;
    void onMoveFiles(movable, folderPath).then(clearSelection);
  }

  // ---- Dialog wiring ----

  async function handleRename(newName: string) {
    if (dialog.kind !== 'rename') return;
    const file = dialog.file;
    const parts = file.path.split('/');
    parts[parts.length - 1] = newName;
    const newPath = parts.join('/');
    await onRenameFile(file.path, newPath);
  }

  async function handleRenameFolder(newName: string) {
    if (dialog.kind !== 'rename-folder') return;
    const folder = dialog.folder;
    const parts = folder.path.split('/');
    parts[parts.length - 1] = newName;
    const newPath = parts.join('/');
    // Backend's /api/files/move handles both files and folders.
    await onRenameFile(folder.path, newPath);
  }

  async function handleCreateFolderDialog(parent: string, name: string) {
    const path = parent ? `${parent}/${name}` : name;
    await onCreateFolder(path);
  }

  async function handleMove(destinationFolder: string) {
    if (dialog.kind !== 'move') return;
    await onMoveFiles(dialog.paths, destinationFolder);
    clearSelection();
  }

  async function handleSetTags(tags: string[]) {
    if (dialog.kind !== 'tags') return;
    await onSetTags(dialog.paths, tags);
  }

  async function handleDelete() {
    if (dialog.kind !== 'delete') return;
    await onDeleteFiles(dialog.paths);
    clearSelection();
  }

  // ---- Render ----

  const dialogInitialTags = useMemo(() => {
    if (dialog.kind !== 'tags') return [];
    return intersectionTags(dialog.paths, meta);
  }, [dialog, meta]);

  const dialogSampleName = useMemo(() => {
    if (dialog.kind !== 'delete' || dialog.paths.length !== 1) return undefined;
    return dialog.paths[0].split('/').pop();
  }, [dialog]);

  const renameSiblings = useMemo(() => {
    if (dialog.kind !== 'rename') return new Set<string>();
    return new Set(
      siblingsOf(dialog.file.path, tree).map((s) => s.toLowerCase()),
    );
  }, [dialog, tree]);

  const folderRenameSiblings = useMemo(() => {
    if (dialog.kind !== 'rename-folder') return new Set<string>();
    return new Set(
      siblingsOf(dialog.folder.path, tree).map((s) => s.toLowerCase()),
    );
  }, [dialog, tree]);

  return (
    <div className={styles.root}>
      <WindowChrome
        title="SpaceIO · my space"
        right={
          <>
            <span className={styles.chromeCount}>{totalFiles} items</span>
            {onToggleTheme && (
              <button
                type="button"
                className={styles.themeBtn}
                onClick={onToggleTheme}
                aria-label={theme === 'dark' ? 'Switch to light mode' : 'Switch to dark mode'}
                title={theme === 'dark' ? 'Switch to light mode' : 'Switch to dark mode'}
              >
                {theme === 'dark' ? <Moon size={13} /> : <Sun size={13} />}
              </button>
            )}
          </>
        }
      />
      <div className={styles.layout}>
        <HearthRail
          calendar={calendar}
          entries={entries}
          entriesLabel={entriesLabel}
          selectedDay={selectedDay}
          onClearSelectedDay={onClearSelectedDay}
          onNewEntry={onNewEntry}
          onSelectFile={onSelectFile}
          onSelectDay={onSelectDay}
          onOpenVault={() => {
            // already in vault — no-op
          }}
          onOpenPasskey={onOpenPasskey}
          hasPasskey={hasPasskey}
          activeSurface="vault"
        />

        <main className={styles.main}>
          <header className={styles.header}>
            <div className={styles.headerText}>
              <div className={styles.eyebrow}>My space</div>
              <h1 className={styles.title}>
                Everything I've kept,
                <br />
                <em>arranged like a shelf</em>
              </h1>
            </div>
            <div className={styles.views}>
              <button
                type="button"
                className={styles.viewBtn}
                onClick={() => setDialog({ kind: 'create-folder' })}
                title="Create a folder anywhere in the tree"
              >
                <Plus size={12} /> New folder
              </button>
            </div>
          </header>

          {selection.size > 0 && (
            <div className={styles.bulkBar} role="toolbar">
              <span className={styles.bulkCount}>
                {selection.size} selected
              </span>
              <button
                type="button"
                className={styles.bulkBtn}
                onClick={() => setDialog({ kind: 'move', paths: Array.from(selection) })}
              >
                <FolderOpen size={12} /> Move…
              </button>
              <button
                type="button"
                className={styles.bulkBtn}
                onClick={() => setDialog({ kind: 'tags', paths: Array.from(selection) })}
              >
                <Tag size={12} /> Tags…
              </button>
              <button
                type="button"
                className={`${styles.bulkBtn} ${styles.bulkBtnDestructive}`}
                onClick={() => setDialog({ kind: 'delete', paths: Array.from(selection) })}
              >
                <Close size={12} /> Delete
              </button>
              <button
                type="button"
                className={styles.bulkClear}
                onClick={clearSelection}
                title="Clear selection (Esc)"
              >
                Clear
              </button>
            </div>
          )}

          <div className={styles.shelves}>
            {folders.length === 0 && (
              <div className={styles.empty}>
                Your space is empty. Press <em>New entry</em> to write your first note.
              </div>
            )}
            {shelfFiles.map(({ folderPath, files }, si) => {
              const folder = folders[si];
              const visible = files.slice(0, 12);
              const isDropTarget = dropTarget === folderPath;
              return (
                <section
                  key={folder.path}
                  className={`${styles.shelf} ${isDropTarget ? styles.shelfDrop : ''}`}
                  onDragOver={(e) => onShelfDragOver(e, folderPath)}
                  onDragLeave={() => onShelfDragLeave(folderPath)}
                  onDrop={(e) => onShelfDrop(e, folderPath)}
                >
                  <div className={styles.shelfHead}>
                    <h2 className={styles.shelfTitle}>
                      <span className={styles.shelfRoman}>{romanNumeral(si + 1)}.</span>{' '}
                      {folder.name}
                    </h2>
                    <span className={styles.shelfMeta}>
                      — {files.length} {files.length === 1 ? 'item' : 'items'}
                    </span>
                    <span className={styles.shelfRule} />
                    {files.length > visible.length && (
                      <button type="button" className={styles.shelfMore} onClick={onBackToReader}>
                        see all →
                      </button>
                    )}
                    <button
                      type="button"
                      className={styles.shelfMenuBtn}
                      onClick={(e) => {
                        const r = e.currentTarget.getBoundingClientRect();
                        openFolderMenu(folder, r.right, r.bottom);
                      }}
                      aria-label={`Manage ${folder.name}`}
                      title="Rename, move, or delete this folder"
                    >
                      <MoreHorizontal size={14} />
                    </button>
                  </div>

                  {visible.length === 0 ? (
                    <div className={styles.shelfEmpty}>
                      <em>Nothing here yet.</em> Drag files in, or use New entry / upload.
                    </div>
                  ) : (
                    <div className={styles.grid}>
                      {visible.map((file) => (
                        <HearthCard
                          key={file.path}
                          file={file}
                          excerpt={excerpts[file.path]}
                          tags={meta[file.path]?.tags}
                          selected={selection.has(file.path)}
                          onOpen={() => onSelectFile(file.path)}
                          onContextMenu={(x, y) => openCardMenu(file, x, y)}
                          onToggleSelect={(mods) => onToggleSelect(file.path, mods)}
                        />
                      ))}
                    </div>
                  )}
                </section>
              );
            })}
          </div>
        </main>
      </div>

      <ContextMenu
        open={menu.open}
        x={menu.x}
        y={menu.y}
        items={menu.items}
        onClose={() => setMenu(EMPTY_MENU)}
      />

      {dialog.kind === 'rename' && (
        <RenameDialog
          open
          currentName={dialog.file.name}
          siblingNames={renameSiblings}
          onClose={() => setDialog({ kind: 'none' })}
          onRename={handleRename}
        />
      )}
      {dialog.kind === 'rename-folder' && (
        <RenameDialog
          open
          currentName={dialog.folder.name}
          siblingNames={folderRenameSiblings}
          onClose={() => setDialog({ kind: 'none' })}
          onRename={handleRenameFolder}
        />
      )}
      {dialog.kind === 'create-folder' && (
        <CreateFolderDialog
          open
          tree={tree}
          onClose={() => setDialog({ kind: 'none' })}
          onCreate={handleCreateFolderDialog}
        />
      )}
      {dialog.kind === 'move' && (
        <MoveDialog
          open
          tree={tree}
          movingPaths={dialog.paths}
          onClose={() => setDialog({ kind: 'none' })}
          onMove={handleMove}
          onCreateFolder={async (parent, name) => {
            const path = parent ? `${parent}/${name}` : name;
            await onCreateFolder(path);
            return path;
          }}
        />
      )}
      {dialog.kind === 'tags' && (
        <TagsDialog
          open
          initialTags={dialogInitialTags}
          fileCount={dialog.paths.length}
          knownTags={knownTags}
          onClose={() => setDialog({ kind: 'none' })}
          onSave={handleSetTags}
        />
      )}
      {dialog.kind === 'delete' && (
        <DeleteConfirmDialog
          open
          count={dialog.paths.length}
          sampleName={dialogSampleName}
          onClose={() => setDialog({ kind: 'none' })}
          onConfirm={handleDelete}
        />
      )}
    </div>
  );
}

function intersectionTags(paths: string[], meta: MetaMap): string[] {
  if (paths.length === 0) return [];
  const first = meta[paths[0]]?.tags ?? [];
  if (paths.length === 1) return first;
  return first.filter((t) =>
    paths.every((p) => (meta[p]?.tags ?? []).includes(t)),
  );
}

function siblingsOf(targetPath: string, tree: TreeNode[]): string[] {
  const parts = targetPath.split('/');
  const leaf = parts.pop() ?? '';
  const parent = parts.join('/');
  const folder = findFolder(tree, parent);
  if (!folder) return [];
  return folder.children
    .map((c) => (c.type === 'file' ? c.name : c.name))
    .filter((n) => n !== leaf);
}

function findFolder(tree: TreeNode[], path: string): TreeFolder | null {
  if (path === '') {
    // Synthesize a "root" folder so siblingsOf works on top-level items.
    return { type: 'folder', name: '', path: '', children: tree };
  }
  const walk = (nodes: TreeNode[]): TreeFolder | null => {
    for (const n of nodes) {
      if (n.type === 'folder') {
        if (n.path === path) return n;
        const hit = walk(n.children);
        if (hit) return hit;
      }
    }
    return null;
  };
  return walk(tree);
}

function collectFilesUnder(folder: TreeFolder): TreeFile[] {
  const out: TreeFile[] = [];
  const walk = (nodes: TreeNode[]) => {
    for (const n of nodes) {
      if (n.type === 'file') out.push(n);
      else walk(n.children);
    }
  };
  walk(folder.children);
  out.sort((a, b) => new Date(b.updated).getTime() - new Date(a.updated).getTime());
  return out;
}

function countFiles(tree: TreeNode[]): number {
  let n = 0;
  const walk = (nodes: TreeNode[]) => {
    for (const x of nodes) {
      if (x.type === 'file') n += 1;
      else walk(x.children);
    }
  };
  walk(tree);
  return n;
}

function romanNumeral(n: number): string {
  return ['I', 'II', 'III', 'IV', 'V', 'VI', 'VII', 'VIII', 'IX', 'X'][n - 1] ?? String(n);
}
