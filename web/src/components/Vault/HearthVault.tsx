import {
  DragEvent,
  memo,
  useCallback,
  useEffect,
  useMemo,
  useRef,
  useState,
} from 'react';
import { WindowChrome } from '../WindowChrome/WindowChrome';
import { HearthRail } from '../Rail/HearthRail';
import { HearthCard } from './HearthCard';
import {
  Chevron,
  ChevronDown,
  Close,
  Download as DownloadIcon,
  Folder,
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
const SHELF_VISIBLE_LIMIT = 12;
const NESTED_VISIBLE_LIMIT = 6;

interface Props {
  tree: TreeNode[];
  excerpts: ExcerptMap;
  meta: MetaMap;
  calendar: CalendarView;
  entries: TodayEntry[];
  entriesLabel: string;
  selectedDay: number | null;
  onClearSelectedDay: () => void;
  onPickDate: (value: string) => void;
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
const EMPTY_SELECTION: ReadonlySet<string> = new Set<string>();

interface Shelf {
  folder: TreeFolder;
  /** Direct files at this folder level (not recursively nested). */
  files: TreeFile[];
  /** Nested subfolders that have their own collapsible tree sections. */
  subfolders: TreeFolder[];
  /** Total count of ALL files recursively under this shelf. */
  totalCount: number;
}

export function HearthVault({
  tree,
  excerpts,
  meta,
  calendar,
  entries,
  entriesLabel,
  selectedDay,
  onClearSelectedDay,
  onPickDate,
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
  const [selection, setSelection] = useState<ReadonlySet<string>>(EMPTY_SELECTION);
  const [anchor, setAnchor] = useState<string | null>(null);
  const [menu, setMenu] = useState<MenuState>(EMPTY_MENU);
  const [dialog, setDialog] = useState<DialogState>({ kind: 'none' });

  // One tree walk yields the shelves (top-level folder → sorted direct files +
  // subfolders), the total count, and the set of known paths.
  const { shelves, totalFiles, knownPaths } = useMemo(() => {
    const shelves: Shelf[] = [];
    let totalFiles = 0;
    const knownPaths = new Set<string>();
    for (const node of tree) {
      if (node.type !== 'folder') continue;
      const allFiles = collectFilesUnder(node);
      totalFiles += allFiles.length;
      for (const file of allFiles) knownPaths.add(file.path);
      const directFiles = node.children
        .filter((child): child is TreeFile => child.type === 'file')
        .sort((a, b) => Date.parse(b.updated) - Date.parse(a.updated));
      const subfolders = node.children.filter((child): child is TreeFolder => child.type === 'folder');
      shelves.push({ folder: node, files: directFiles, subfolders, totalCount: allFiles.length });
    }
    return { shelves, totalFiles, knownPaths };
  }, [tree]);

  const orderedPaths = useMemo(() => {
    const list: string[] = [];
    for (const shelf of shelves) for (const file of shelf.files) list.push(file.path);
    return list;
  }, [shelves]);

  // Drop deleted/moved files from the selection.
  useEffect(() => {
    setSelection((cur) => {
      if (cur.size === 0) return cur;
      let changed = false;
      const next = new Set<string>();
      for (const path of cur) {
        if (knownPaths.has(path)) next.add(path);
        else changed = true;
      }
      return changed ? next : cur;
    });
  }, [knownPaths]);

  const knownTags = useMemo(() => {
    const set = new Set<string>();
    for (const entry of Object.values(meta)) for (const tag of entry.tags) set.add(tag);
    return Array.from(set).sort((a, b) => a.localeCompare(b));
  }, [meta]);

  // Refs keep the card callbacks referentially stable (so React.memo on
  // HearthCard holds) while still reading the latest values.
  const selectionRef = useRef(selection);
  selectionRef.current = selection;
  const anchorRef = useRef(anchor);
  anchorRef.current = anchor;
  const orderedPathsRef = useRef(orderedPaths);
  orderedPathsRef.current = orderedPaths;

  const clearSelection = useCallback(() => {
    setSelection(EMPTY_SELECTION);
    setAnchor(null);
  }, []);

  const onCardToggleSelect = useCallback(
    (file: TreeFile, mods: { shift?: boolean; cmd?: boolean }) => {
      const path = file.path;
      const cur = selectionRef.current;
      const next = new Set(cur);
      if (mods.shift && anchorRef.current) {
        const paths = orderedPathsRef.current;
        const anchorIndex = paths.indexOf(anchorRef.current);
        const targetIndex = paths.indexOf(path);
        if (anchorIndex >= 0 && targetIndex >= 0) {
          const [lo, hi] =
            anchorIndex < targetIndex ? [anchorIndex, targetIndex] : [targetIndex, anchorIndex];
          for (let i = lo; i <= hi; i += 1) next.add(paths[i]);
          setSelection(next);
          return;
        }
      }
      if (next.has(path)) next.delete(path);
      else next.add(path);
      setSelection(next);
      if (!mods.shift) setAnchor(path);
    },
    [],
  );

  const onCardOpen = useCallback(
    (file: TreeFile) => {
      onSelectFile(file.path);
    },
    [onSelectFile],
  );

  const buildFileMenu = useCallback(
    (file: TreeFile): MenuItem[] => {
      const sel = selectionRef.current;
      const selected = sel.has(file.path);
      const targets = selected && sel.size > 1 ? Array.from(sel) : [file.path];
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
    },
    [onDownloadFile, onSelectFile],
  );

  const onCardContextMenu = useCallback(
    (file: TreeFile, x: number, y: number) => {
      setMenu({ open: true, x, y, items: buildFileMenu(file) });
    },
    [buildFileMenu],
  );

  const onShelfDrop = useCallback(
    (folderPath: string, droppedPath: string) => {
      if (!droppedPath) return;
      const sel = selectionRef.current;
      const targets = sel.has(droppedPath) ? Array.from(sel) : [droppedPath];
      // A target already inside the destination subtree would be a no-op move.
      const prefix = `${folderPath}/`;
      const movable = targets.filter((path) => !path.startsWith(prefix));
      if (movable.length === 0) return;
      void onMoveFiles(movable, folderPath).then(clearSelection);
    },
    [clearSelection, onMoveFiles],
  );

  const onShelfFolderMenu = useCallback(
    (folder: TreeFolder, x: number, y: number) => {
      const items: MenuItem[] = [
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
      setMenu({ open: true, x, y, items });
    },
    [],
  );

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
    // /api/files/move handles both files and folders.
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
    return new Set(siblingsOf(dialog.file.path, tree).map((name) => name.toLowerCase()));
  }, [dialog, tree]);

  const folderRenameSiblings = useMemo(() => {
    if (dialog.kind !== 'rename-folder') return new Set<string>();
    return new Set(siblingsOf(dialog.folder.path, tree).map((name) => name.toLowerCase()));
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
          onPickDate={onPickDate}
          onNewEntry={onNewEntry}
          onSelectFile={onSelectFile}
          onSelectDay={onSelectDay}
          onOpenVault={noop}
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
              <span className={styles.bulkCount}>{selection.size} selected</span>
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
            {shelves.length === 0 && (
              <div className={styles.empty}>
                Your space is empty. Press <em>New entry</em> to write your first note.
              </div>
            )}
            {shelves.map(({ folder, files, subfolders, totalCount }, si) => (
              <VaultShelf
                key={folder.path}
                folder={folder}
                files={files}
                subfolders={subfolders}
                totalCount={totalCount}
                index={si}
                excerpts={excerpts}
                meta={meta}
                selection={selection}
                onCardOpen={onCardOpen}
                onCardContextMenu={onCardContextMenu}
                onCardToggleSelect={onCardToggleSelect}
                onDropFile={onShelfDrop}
                onBackToReader={onBackToReader}
                onFolderMenu={onShelfFolderMenu}
              />
            ))}
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

function noop() {}

interface FolderDropTarget {
  isDropTarget: boolean;
  dragProps: {
    onDragEnter: (event: DragEvent<HTMLElement>) => void;
    onDragOver: (event: DragEvent<HTMLElement>) => void;
    onDragLeave: (event: DragEvent<HTMLElement>) => void;
    onDrop: (event: DragEvent<HTMLElement>) => void;
  };
}

/** Drag-and-drop handlers for a folder drop zone. Each zone tracks its own
 *  highlight so a dragover only re-renders that zone. Nested zones pass
 *  `stopPropagation` so a drop lands on the innermost folder. */
function useFolderDropTarget(
  folderPath: string,
  onDropFile: (folderPath: string, droppedPath: string) => void,
  options?: { stopPropagation?: boolean },
): FolderDropTarget {
  const [isDropTarget, setIsDropTarget] = useState(false);
  const dragDepth = useRef(0);
  const stopPropagation = options?.stopPropagation ?? false;

  const dragProps = {
    onDragEnter(event: DragEvent<HTMLElement>) {
      if (!hasHearthDrag(event)) return;
      event.preventDefault();
      if (stopPropagation) event.stopPropagation();
      dragDepth.current += 1;
      if (dragDepth.current === 1) setIsDropTarget(true);
    },
    onDragOver(event: DragEvent<HTMLElement>) {
      if (!hasHearthDrag(event)) return;
      event.preventDefault();
      if (stopPropagation) event.stopPropagation();
      event.dataTransfer.dropEffect = 'move';
    },
    onDragLeave(event: DragEvent<HTMLElement>) {
      if (stopPropagation) event.stopPropagation();
      dragDepth.current = Math.max(0, dragDepth.current - 1);
      if (dragDepth.current === 0) setIsDropTarget(false);
    },
    onDrop(event: DragEvent<HTMLElement>) {
      event.preventDefault();
      if (stopPropagation) event.stopPropagation();
      dragDepth.current = 0;
      setIsDropTarget(false);
      onDropFile(folderPath, event.dataTransfer.getData(DRAG_MIME));
    },
  };

  return { isDropTarget, dragProps };
}

interface VaultShelfProps {
  folder: TreeFolder;
  files: TreeFile[];
  subfolders: TreeFolder[];
  totalCount: number;
  index: number;
  excerpts: ExcerptMap;
  meta: MetaMap;
  selection: ReadonlySet<string>;
  onCardOpen: (file: TreeFile) => void;
  onCardContextMenu: (file: TreeFile, x: number, y: number) => void;
  onCardToggleSelect: (file: TreeFile, mods: { shift?: boolean; cmd?: boolean }) => void;
  onDropFile: (folderPath: string, droppedPath: string) => void;
  onBackToReader: () => void;
  onFolderMenu: (folder: TreeFolder, x: number, y: number) => void;
}

const VaultShelf = memo(function VaultShelf({
  folder,
  files,
  subfolders,
  totalCount,
  index,
  excerpts,
  meta,
  selection,
  onCardOpen,
  onCardContextMenu,
  onCardToggleSelect,
  onDropFile,
  onBackToReader,
  onFolderMenu,
}: VaultShelfProps) {
  const { isDropTarget, dragProps } = useFolderDropTarget(folder.path, onDropFile);

  const visible = files.length > SHELF_VISIBLE_LIMIT ? files.slice(0, SHELF_VISIBLE_LIMIT) : files;

  return (
    <section className={`${styles.shelf} ${isDropTarget ? styles.shelfDrop : ''}`} {...dragProps}>
      <div className={styles.shelfHead}>
        <h2 className={styles.shelfTitle}>
          <span className={styles.shelfRoman}>{romanNumeral(index + 1)}.</span>{' '}
          {folder.name}
        </h2>
        <span className={styles.shelfMeta}>
          — {totalCount} {totalCount === 1 ? 'item' : 'items'}
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
          onClick={(event) => {
            const rect = event.currentTarget.getBoundingClientRect();
            onFolderMenu(folder, rect.right, rect.bottom);
          }}
          aria-label={`Manage ${folder.name}`}
          title="Rename, move, or delete this folder"
        >
          <MoreHorizontal size={14} />
        </button>
      </div>

      {visible.length === 0 && subfolders.length === 0 ? (
        <div className={styles.shelfEmpty}>
          <em>Nothing here yet.</em> Drag files in, or use New entry / upload.
        </div>
      ) : (
        <>
          {visible.length > 0 && (
            <div className={styles.grid}>
              {visible.map((file) => (
                <HearthCard
                  key={file.path}
                  file={file}
                  excerpt={excerpts[file.path]}
                  tags={meta[file.path]?.tags}
                  selected={selection.has(file.path)}
                  onOpen={onCardOpen}
                  onContextMenu={onCardContextMenu}
                  onToggleSelect={onCardToggleSelect}
                />
              ))}
            </div>
          )}
          {subfolders.map((sub) => (
            <NestedFolder
              key={sub.path}
              folder={sub}
              depth={1}
              excerpts={excerpts}
              meta={meta}
              selection={selection}
              onCardOpen={onCardOpen}
              onCardContextMenu={onCardContextMenu}
              onCardToggleSelect={onCardToggleSelect}
              onDropFile={onDropFile}
              onFolderMenu={onFolderMenu}
            />
          ))}
        </>
      )}
    </section>
  );
});

interface NestedFolderProps {
  folder: TreeFolder;
  depth: number;
  excerpts: ExcerptMap;
  meta: MetaMap;
  selection: ReadonlySet<string>;
  onCardOpen: (file: TreeFile) => void;
  onCardContextMenu: (file: TreeFile, x: number, y: number) => void;
  onCardToggleSelect: (file: TreeFile, mods: { shift?: boolean; cmd?: boolean }) => void;
  onDropFile: (folderPath: string, droppedPath: string) => void;
  onFolderMenu: (folder: TreeFolder, x: number, y: number) => void;
}

/** Renders a collapsible nested folder within a shelf. Subfolders at depth > 0
 *  are collapsed by default; expanding reveals direct files and deeper subfolders. */
const NestedFolder = memo(function NestedFolder({
  folder,
  depth,
  excerpts,
  meta,
  selection,
  onCardOpen,
  onCardContextMenu,
  onCardToggleSelect,
  onDropFile,
  onFolderMenu,
}: NestedFolderProps) {
  const [expanded, setExpanded] = useState(false);
  const { isDropTarget, dragProps } = useFolderDropTarget(folder.path, onDropFile, {
    stopPropagation: true,
  });

  const directFiles = useMemo(
    () =>
      folder.children
        .filter((child): child is TreeFile => child.type === 'file')
        .sort((a, b) => Date.parse(b.updated) - Date.parse(a.updated)),
    [folder.children],
  );
  const subfolders = useMemo(
    () => folder.children.filter((child): child is TreeFolder => child.type === 'folder'),
    [folder.children],
  );

  const totalCount = useMemo(() => countFilesUnder(folder), [folder]);
  const visibleFiles = directFiles.length > NESTED_VISIBLE_LIMIT
    ? directFiles.slice(0, NESTED_VISIBLE_LIMIT)
    : directFiles;

  return (
    <div
      className={`${styles.nestedFolder} ${isDropTarget ? styles.nestedFolderDrop : ''}`}
      style={{ paddingLeft: `${depth * 16}px` }}
      {...dragProps}
    >
      <div className={styles.nestedFolderHead}>
        <button
          type="button"
          className={styles.nestedFolderToggle}
          onClick={() => setExpanded((current) => !current)}
          aria-expanded={expanded}
          aria-label={`${expanded ? 'Collapse' : 'Expand'} folder ${folder.name}`}
        >
          {expanded ? <ChevronDown size={12} /> : <Chevron size={12} />}
        </button>
        <span className={styles.nestedFolderIcon}>
          {expanded ? <FolderOpen size={14} /> : <Folder size={14} />}
        </span>
        <span className={styles.nestedFolderName}>{folder.name}</span>
        <span className={styles.nestedFolderCount}>
          {totalCount} {totalCount === 1 ? 'item' : 'items'}
        </span>
        <button
          type="button"
          className={styles.shelfMenuBtn}
          onClick={(event) => {
            const rect = event.currentTarget.getBoundingClientRect();
            onFolderMenu(folder, rect.right, rect.bottom);
          }}
          aria-label={`Manage ${folder.name}`}
          title="Rename, move, or delete this folder"
        >
          <MoreHorizontal size={14} />
        </button>
      </div>

      {expanded && (
        <div className={styles.nestedFolderContent}>
          {visibleFiles.length > 0 && (
            <div className={styles.grid}>
              {visibleFiles.map((file) => (
                <HearthCard
                  key={file.path}
                  file={file}
                  excerpt={excerpts[file.path]}
                  tags={meta[file.path]?.tags}
                  selected={selection.has(file.path)}
                  onOpen={onCardOpen}
                  onContextMenu={onCardContextMenu}
                  onToggleSelect={onCardToggleSelect}
                />
              ))}
            </div>
          )}
          {directFiles.length > visibleFiles.length && (
            <div className={styles.nestedFolderOverflow}>
              +{directFiles.length - visibleFiles.length} more items
            </div>
          )}
          {subfolders.map((sub) => (
            <NestedFolder
              key={sub.path}
              folder={sub}
              depth={depth + 1}
              excerpts={excerpts}
              meta={meta}
              selection={selection}
              onCardOpen={onCardOpen}
              onCardContextMenu={onCardContextMenu}
              onCardToggleSelect={onCardToggleSelect}
              onDropFile={onDropFile}
              onFolderMenu={onFolderMenu}
            />
          ))}
          {visibleFiles.length === 0 && subfolders.length === 0 && (
            <div className={styles.shelfEmpty}>
              <em>Empty folder.</em>
            </div>
          )}
        </div>
      )}
    </div>
  );
});

function hasHearthDrag(event: DragEvent<HTMLElement>): boolean {
  const types = event.dataTransfer?.types;
  if (!types) return false;
  for (let i = 0; i < types.length; i += 1) {
    if (types[i] === DRAG_MIME) return true;
  }
  return false;
}

function intersectionTags(paths: string[], meta: MetaMap): string[] {
  if (paths.length === 0) return [];
  const first = meta[paths[0]]?.tags ?? [];
  if (paths.length === 1) return first;
  return first.filter((tag) => paths.every((path) => (meta[path]?.tags ?? []).includes(tag)));
}

function siblingsOf(targetPath: string, tree: TreeNode[]): string[] {
  const parts = targetPath.split('/');
  const leaf = parts.pop() ?? '';
  const parent = parts.join('/');
  const folder = findFolder(tree, parent);
  if (!folder) return [];
  return folder.children.map((child) => child.name).filter((name) => name !== leaf);
}

function findFolder(tree: TreeNode[], path: string): TreeFolder | null {
  if (path === '') {
    return { type: 'folder', name: '', path: '', children: tree };
  }
  const walk = (nodes: TreeNode[]): TreeFolder | null => {
    for (const node of nodes) {
      if (node.type === 'folder') {
        if (node.path === path) return node;
        const hit = walk(node.children);
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
    for (const node of nodes) {
      if (node.type === 'file') out.push(node);
      else walk(node.children);
    }
  };
  walk(folder.children);
  out.sort((a, b) => Date.parse(b.updated) - Date.parse(a.updated));
  return out;
}

/** Count total files recursively under a folder (without allocating an array). */
function countFilesUnder(folder: TreeFolder): number {
  let count = 0;
  const walk = (nodes: TreeNode[]) => {
    for (const node of nodes) {
      if (node.type === 'file') count += 1;
      else walk(node.children);
    }
  };
  walk(folder.children);
  return count;
}

const ROMAN = ['I', 'II', 'III', 'IV', 'V', 'VI', 'VII', 'VIII', 'IX', 'X'];

function romanNumeral(value: number): string {
  return ROMAN[value - 1] ?? String(value);
}
