import { DragEvent, useCallback, useEffect, useMemo, useRef, useState } from 'react';
import { Auth } from './components/Auth/Auth';
import { Registration } from './components/Registration/Registration';
import { Reader } from './components/Reader/Reader';
import { SpaceVault } from './components/Vault/SpaceVault';
import { Preview } from './components/Preview/Preview';
import { SearchOverlay } from './components/Search/SearchOverlay';
import { UploadModal } from './components/Upload/UploadModal';
import { DownloadModal } from './components/Download/DownloadModal';
import { PasskeyModal } from './components/Passkey/PasskeyModal';
import { AgentChat } from './components/Agent/AgentChat';
import { Sparkle } from './components/icons/Icon';
import {
  api,
  ExcerptMap,
  firstMarkdownLeaf,
  MetaMap,
  ReadFile,
  TreeFile,
  TreeNode,
} from './api/client';
import {
  buildCalendar,
  CalendarView,
  entriesForDate,
  shortDayLabel,
  TodayEntry,
} from './lib/calendar';

type View =
  | { kind: 'loading' }
  | { kind: 'registration'; anyUsers: boolean }
  | { kind: 'auth'; anyUsers: boolean }
  | { kind: 'unlocked'; owner: string; email: string; surface: Surface }
  | { kind: 'fatal'; message: string };

type Surface =
  | { kind: 'reader'; file: ReadFile; initialMode: 'preview' | 'edit' }
  | { kind: 'vault'; previousPath: string | null }
  | { kind: 'preview'; file: TreeFile; previousPath: string | null };

const DEFAULT_NEW_FOLDER = (() => `Journal/${new Date().getFullYear()}`)();

export function App() {
  const [view, setView] = useState<View>({ kind: 'loading' });
  const [tree, setTree] = useState<TreeNode[]>([]);
  const [excerpts, setExcerpts] = useState<ExcerptMap>({});
  const [meta, setMeta] = useState<MetaMap>({});
  const [todayDate, setTodayDate] = useState(() => startOfDay(new Date()));
  const [viewMonth, setViewMonth] = useState(() => startOfMonth(new Date()));
  const [hasPasskey, setHasPasskey] = useState(false);
  const [searchOpen, setSearchOpen] = useState(false);
  const [uploadOpen, setUploadOpen] = useState(false);
  const [uploadInitial, setUploadInitial] = useState<File[] | undefined>(undefined);
  const [downloadFile, setDownloadFile] = useState<TreeFile | null>(null);
  const [passkeyOpen, setPasskeyOpen] = useState(false);
  const [agentOpen, setAgentOpen] = useState(false);
  const [toast, setToast] = useState<string | null>(null);
  const [selectedDay, setSelectedDay] = useState<number | null>(null);
  const [theme, setTheme] = useState<'light' | 'dark'>(() => {
    const stored = typeof window !== 'undefined' ? window.localStorage.getItem('space-io.theme') : null;
    return stored === 'dark' ? 'dark' : 'light';
  });

  useEffect(() => {
    if (typeof document === 'undefined') { return; }
    document.documentElement.dataset.theme = theme;
    window.localStorage.setItem('space-io.theme', theme);
  }, [theme]);

  const toggleTheme = useCallback(() => {
    setTheme((current) => (current === 'dark' ? 'light' : 'dark'));
  }, []);

  useEffect(() => {
    const tick = () => {
      const next = startOfDay(new Date());
      setTodayDate((prev) => (prev.getTime() === next.getTime() ? prev : next));
    };
    const timer = window.setInterval(tick, 60_000);
    return () => window.clearInterval(timer);
  }, []);

  const previousPathRef = useRef<string | null>(null);

  const refreshTree = useCallback(async () => {
    const { tree } = await api.tree();
    setTree(tree);
    return tree;
  }, []);

  const refreshExcerpts = useCallback(async () => {
    try {
      const { excerpts } = await api.excerpts();
      setExcerpts(excerpts);
    } catch (err) {
      console.error('excerpts failed', err);
    }
  }, []);

  const refreshMeta = useCallback(async () => {
    try {
      const { meta } = await api.meta();
      setMeta(meta);
    } catch (err) {
      console.error('meta failed', err);
    }
  }, []);

  const refreshStatus = useCallback(async () => {
    try {
      const status = await api.status();
      setHasPasskey(status.has_passkey);
      return status;
    } catch (err) {
      console.error('status failed', err);
      return null;
    }
  }, []);

  const enterReader = useCallback(
    async (owner: string, email: string) => {
      try {
        const loadedTree = await refreshTree();
        void refreshExcerpts();
        void refreshMeta();
        const leaf = firstMarkdownLeaf(loadedTree);
        if (!leaf) {
          const { path } = await api.create(DEFAULT_NEW_FOLDER);
          await refreshTree();
          const file: ReadFile = { path, content: '', updated: null };
          previousPathRef.current = path;
          setView({
            kind: 'unlocked',
            owner,
            email,
            surface: { kind: 'reader', file, initialMode: 'edit' },
          });
          return;
        }
        const file = await api.read(leaf.path);
        previousPathRef.current = file.path;
        setView({
          kind: 'unlocked',
          owner,
          email,
          surface: { kind: 'reader', file, initialMode: 'preview' },
        });
      } catch (err) {
        setView({
          kind: 'fatal',
          message: err instanceof Error ? err.message : 'Failed to load the space.',
        });
      }
    },
    [refreshExcerpts, refreshMeta, refreshTree],
  );

  useEffect(() => {
    let cancelled = false;
    (async () => {
      try {
        const status = await api.status();
        if (cancelled) { return; }
        setHasPasskey(status.has_passkey);
        if (status.unlocked) {
          await enterReader(status.owner, status.email);
        } else if (!status.any_users) {
          setView({ kind: 'registration', anyUsers: false });
        } else {
          setView({ kind: 'auth', anyUsers: true });
        }
      } catch (err) {
        if (cancelled) { return; }
        setView({
          kind: 'fatal',
          message: err instanceof Error ? err.message : 'Could not reach the server.',
        });
      }
    })();
    return () => {
      cancelled = true;
    };
  }, [enterReader]);

  const onRegistered = useCallback(async () => {
    try {
      const status = await api.status();
      setHasPasskey(status.has_passkey);
      if (status.unlocked) {
        await enterReader(status.owner, status.email);
      } else {
        setView({ kind: 'auth', anyUsers: status.any_users });
      }
    } catch (err) {
      setView({
        kind: 'fatal',
        message: err instanceof Error ? err.message : 'Registration succeeded but the next step failed.',
      });
    }
  }, [enterReader]);

  const onUnlocked = useCallback(async () => {
    if (view.kind !== 'auth') { return; }
    setView({ kind: 'loading' });
    try {
      const status = await api.status();
      setHasPasskey(status.has_passkey);
      await enterReader(status.owner, status.email);
    } catch (err) {
      setView({
        kind: 'fatal',
        message: err instanceof Error ? err.message : 'Unlock succeeded but the next step failed.',
      });
    }
  }, [enterReader, view]);

  const showRegistration = useCallback(() => {
    setView({ kind: 'registration', anyUsers: true });
  }, []);

  const showLogin = useCallback(() => {
    setView({ kind: 'auth', anyUsers: true });
  }, []);

  /** Lock the session. The server call is best-effort: local state is cleared
   *  regardless of whether it succeeds. */
  const onLock = useCallback(async () => {
    try {
      await api.lock();
    } catch {}
    setTree([]);
    setExcerpts({});
    setMeta({});
    previousPathRef.current = null;
    setSearchOpen(false);
    setUploadOpen(false);
    setUploadInitial(undefined);
    setDownloadFile(null);
    setPasskeyOpen(false);
    setAgentOpen(false);
    setSelectedDay(null);
    setToast(null);
    const status = await refreshStatus();
    setView({
      kind: 'auth',
      anyUsers: status?.any_users ?? true,
    });
  }, [refreshStatus]);

  const selectFile = useCallback(
    async (path: string) => {
      if (view.kind !== 'unlocked') { return; }
      try {
        const file = await api.read(path);
        previousPathRef.current = path;
        setView({
          kind: 'unlocked',
          owner: view.owner,
          email: view.email,
          surface: { kind: 'reader', file, initialMode: 'preview' },
        });
        setSearchOpen(false);
      } catch (err) {
        console.error('failed to read file', err);
        setToast(err instanceof Error ? err.message : 'Could not open file');
      }
    },
    [view],
  );

  const openPreview = useCallback(
    (file: TreeFile) => {
      if (view.kind !== 'unlocked') { return; }
      const previous = previousPathRef.current;
      setView({
        kind: 'unlocked',
        owner: view.owner,
        email: view.email,
        surface: { kind: 'preview', file, previousPath: previous },
      });
    },
    [view],
  );

  const openVault = useCallback(async () => {
    if (view.kind !== 'unlocked') { return; }
    setView({
      kind: 'unlocked',
      owner: view.owner,
      email: view.email,
      surface: { kind: 'vault', previousPath: previousPathRef.current },
    });
    try {
      await Promise.all([refreshTree(), refreshExcerpts(), refreshMeta()]);
    } catch (err) {
      console.error('failed to refresh vault', err);
    }
  }, [refreshExcerpts, refreshMeta, refreshTree, view]);

  const backFromVault = useCallback(async () => {
    if (view.kind !== 'unlocked') { return; }
    if (view.surface.kind !== 'vault') { return; }
    const target = view.surface.previousPath;
    if (!target) {
      void enterReader(view.owner, view.email);
      return;
    }
    try {
      const file = await api.read(target);
      setView({
        kind: 'unlocked',
        owner: view.owner,
        email: view.email,
        surface: { kind: 'reader', file, initialMode: 'preview' },
      });
    } catch {
      void enterReader(view.owner, view.email);
    }
  }, [enterReader, view]);

  const newEntry = useCallback(async () => {
    if (view.kind !== 'unlocked') { return; }
    try {
      const { path } = await api.create(DEFAULT_NEW_FOLDER);
      const file = await api.read(path);
      await refreshTree();
      void refreshExcerpts();
      previousPathRef.current = path;
      setView({
        kind: 'unlocked',
        owner: view.owner,
        email: view.email,
        surface: { kind: 'reader', file, initialMode: 'edit' },
      });
    } catch (err) {
      console.error('failed to create new entry', err);
      setToast(err instanceof Error ? err.message : 'Could not create new entry');
    }
  }, [refreshExcerpts, refreshTree, view]);

  /** Patch the local title/excerpt for `path` so the Today list and wikilink
   *  autocomplete update without a full server re-walk on every keystroke. */
  const patchExcerpt = useCallback((path: string, content: string) => {
    const titleMatch = /^# (.+)$/m.exec(content);
    const title = titleMatch ? titleMatch[1].trim() : null;
    const bodyLines = content
      .split('\n')
      .filter((line) => !line.startsWith('#') && line.trim().length > 0)
      .slice(0, 3)
      .join(' ');
    const excerpt = bodyLines
      .replace(/[*_`]/g, '')
      .replace(/\[\[|\]\]/g, '')
      .slice(0, 180);
    setExcerpts((cur) => ({ ...cur, [path]: { title, excerpt } }));
  }, []);

  /** Autosave: persist the draft without a history entry. */
  const saveFile = useCallback(
    async (path: string, content: string) => {
      await api.saveDraft(path, content);
      patchExcerpt(path, content);
    },
    [patchExcerpt],
  );

  /** Checkpoint: persist and record a labelled point in the version history. */
  const checkpointFile = useCallback(
    async (path: string, content: string, message?: string) => {
      await api.checkpoint(path, content, message);
      patchExcerpt(path, content);
    },
    [patchExcerpt],
  );

  const rollbackFile = useCallback(
    async (path: string, commit: string) => {
      if (view.kind !== 'unlocked') { return; }
      await api.rollback(path, commit);
      const file = await api.read(path);
      void refreshTree();
      void refreshExcerpts();
      previousPathRef.current = path;
      setView({
        kind: 'unlocked',
        owner: view.owner,
        email: view.email,
        surface: { kind: 'reader', file, initialMode: 'preview' },
      });
    },
    [refreshExcerpts, refreshTree, view],
  );

  const onSelectVaultFile = useCallback(
    (file: TreeFile) => {
      if (file.kind === 'md') {
        void selectFile(file.path);
      } else {
        openPreview(file);
      }
    },
    [openPreview, selectFile],
  );

  const selectDay = useCallback((day: number) => {
    setSelectedDay((cur) => (cur === day ? null : day));
  }, []);

  const clearSelectedDay = useCallback(() => setSelectedDay(null), []);

  const pickDate = useCallback((value: string) => {
    const [year, month, day] = value.split('-').map(Number);
    if (!Number.isFinite(year) || !Number.isFinite(month) || !Number.isFinite(day)) { return; }
    setViewMonth(new Date(year, month - 1, 1));
    setSelectedDay(day);
  }, []);

  /** Resolve a path to its `TreeFile` (for preview routing) and select it. Kept
   *  referentially stable so the memoized rail doesn't re-render needlessly. */
  const onVaultSelectPath = useCallback(
    (path: string) => {
      const file = findInTree(tree, path);
      if (file) { onSelectVaultFile(file); }
      else { void selectFile(path); }
    },
    [onSelectVaultFile, selectFile, tree],
  );

  const openSearch = useCallback(() => setSearchOpen(true), []);
  const openPasskey = useCallback(() => setPasskeyOpen(true), []);

  const onAgentVaultChanged = useCallback(() => {
    void refreshTree();
    void refreshExcerpts();
    void refreshMeta();
  }, [refreshExcerpts, refreshMeta, refreshTree]);

  const onUploaded = useCallback(async () => {
    await refreshTree();
    void refreshExcerpts();
  }, [refreshExcerpts, refreshTree]);

  const handleRenameFile = useCallback(
    async (from: string, to: string) => {
      try {
        await api.move(from, to);
        await refreshTree();
        void refreshMeta();
      } catch (err) {
        setToast(err instanceof Error ? err.message : 'rename failed');
        throw err;
      }
    },
    [refreshMeta, refreshTree],
  );

  const handleMoveFiles = useCallback(
    async (paths: string[], destinationFolder: string) => {
      try {
        const moves = paths
          .map((from) => {
            const name = from.split('/').pop() ?? from;
            const to = destinationFolder ? `${destinationFolder}/${name}` : name;
            return { from, to };
          })
          .filter((move) => move.from !== move.to);
        if (moves.length === 0) { return; }
        await api.moveBulk(moves);
        await refreshTree();
        void refreshMeta();
      } catch (err) {
        await refreshTree();
        void refreshMeta();
        setToast(err instanceof Error ? err.message : 'move failed');
        throw err;
      }
    },
    [refreshMeta, refreshTree],
  );

  const handleCreateFolder = useCallback(
    async (path: string) => {
      try {
        await api.mkdir(path);
        await refreshTree();
      } catch (err) {
        setToast(err instanceof Error ? err.message : 'mkdir failed');
        throw err;
      }
    },
    [refreshTree],
  );

  const handleDeleteFiles = useCallback(
    async (paths: string[]) => {
      if (paths.length === 0) { return; }
      try {
        await api.deleteFilesBulk(paths);
        await refreshTree();
        void refreshExcerpts();
        void refreshMeta();
      } catch (err) {
        await refreshTree();
        setToast(err instanceof Error ? err.message : 'delete failed');
        throw err;
      }
    },
    [refreshExcerpts, refreshMeta, refreshTree],
  );

  const handleSetTags = useCallback(
    async (paths: string[], tags: string[]) => {
      if (paths.length === 0) { return; }
      try {
        await api.setTagsBulk(paths.map((path) => ({ path, tags })));
        setMeta((cur) => {
          const next = { ...cur };
          for (const path of paths) {
            if (tags.length === 0) { delete next[path]; }
            else { next[path] = { tags }; }
          }
          return next;
        });
      } catch (err) {
        void refreshMeta();
        setToast(err instanceof Error ? err.message : 'tag update failed');
        throw err;
      }
    },
    [refreshMeta],
  );

  const onWikilinkMiss = useCallback((title: string) => {
    setToast(`No note titled "${title}" — yet.`);
  }, []);

  useEffect(() => {
    function onKey(event: KeyboardEvent) {
      if ((event.metaKey || event.ctrlKey) && event.key.toLowerCase() === 'k') {
        if (view.kind === 'unlocked') {
          event.preventDefault();
          setSearchOpen((open) => !open);
        }
      } else if (event.key === 'Escape') {
        if (searchOpen) { setSearchOpen(false); }
      }
    }
    window.addEventListener('keydown', onKey);
    return () => window.removeEventListener('keydown', onKey);
  }, [searchOpen, view]);

  const dragCounter = useRef(0);
  const [dragOverlay, setDragOverlay] = useState(false);
  const handleWindowDragEnter = useCallback(
    (event: DragEvent) => {
      if (view.kind !== 'unlocked') { return; }
      if (!hasFiles(event)) { return; }
      event.preventDefault();
      dragCounter.current += 1;
      setDragOverlay(true);
    },
    [view],
  );
  const handleWindowDragLeave = useCallback((event: DragEvent) => {
    event.preventDefault();
    dragCounter.current -= 1;
    if (dragCounter.current <= 0) {
      dragCounter.current = 0;
      setDragOverlay(false);
    }
  }, []);
  const handleWindowDrop = useCallback(
    (event: DragEvent) => {
      if (view.kind !== 'unlocked') { return; }
      if (!hasFiles(event)) { return; }
      event.preventDefault();
      dragCounter.current = 0;
      setDragOverlay(false);
      const files = Array.from(event.dataTransfer?.files ?? []);
      if (files.length === 0) { return; }
      setUploadInitial(files);
      setUploadOpen(true);
    },
    [view],
  );

  useEffect(() => {
    if (!toast) { return; }
    const timer = window.setTimeout(() => setToast(null), 3200);
    return () => window.clearTimeout(timer);
  }, [toast]);

  const calendar: CalendarView = useMemo(
    () => buildCalendar(viewMonth, todayDate, tree),
    [viewMonth, todayDate, tree],
  );
  const isCurrentMonth = calendar.today > 0;
  useEffect(() => {
    if (selectedDay != null && selectedDay > calendar.daysInMonth) {
      setSelectedDay(null);
    }
  }, [calendar.daysInMonth, selectedDay]);

  const currentPath =
    view.kind === 'unlocked' && view.surface.kind === 'reader' ? view.surface.file.path : null;
  const railEntries: TodayEntry[] = useMemo(() => {
    const target =
      selectedDay != null
        ? new Date(calendar.year, calendar.month, selectedDay)
        : isCurrentMonth
          ? todayDate
          : null;
    return target ? entriesForDate(target, tree, excerpts, currentPath) : [];
  }, [
    todayDate,
    selectedDay,
    isCurrentMonth,
    calendar.year,
    calendar.month,
    tree,
    excerpts,
    currentPath,
  ]);
  const railLabel: string =
    selectedDay != null
      ? shortDayLabel(calendar.month, selectedDay)
      : isCurrentMonth
        ? 'Today'
        : calendar.monthLabel;
  const titleToPath = useMemo(() => buildTitleMap(tree, excerpts), [tree, excerpts]);

  if (view.kind === 'loading') { return <LoadingScreen />; }
  if (view.kind === 'registration') {
    return (
      <Registration
        showBackToLogin={view.anyUsers}
        onRegistered={onRegistered}
        onBackToLogin={showLogin}
      />
    );
  }
  if (view.kind === 'auth') {
    return (
      <Auth
        showRegisterLink={view.anyUsers}
        onUnlocked={onUnlocked}
        onRegister={showRegistration}
      />
    );
  }
  if (view.kind === 'fatal') { return <FatalScreen message={view.message} />; }

  const { surface } = view;
  return (
    <div
      onDragEnter={handleWindowDragEnter}
      onDragOver={(event) => {
        if (hasFiles(event)) { event.preventDefault(); }
      }}
      onDragLeave={handleWindowDragLeave}
      onDrop={handleWindowDrop}
      style={{ position: 'absolute', inset: 0 }}
    >
      {surface.kind === 'reader' && (
        <Reader
          path={surface.file.path}
          content={surface.file.content}
          updated={surface.file.updated}
          initialMode={surface.initialMode}
          calendar={calendar}
          entries={railEntries}
          entriesLabel={railLabel}
          selectedDay={selectedDay}
          onClearSelectedDay={clearSelectedDay}
          onPickDate={pickDate}
          titleToPath={titleToPath}
          onSelectFile={selectFile}
          onSelectDay={selectDay}
          onNewEntry={newEntry}
          onOpenVault={openVault}
          onOpenSearch={openSearch}
          onLock={onLock}
          onSave={saveFile}
          onCheckpoint={checkpointFile}
          onRollback={rollbackFile}
          onWikilinkMiss={onWikilinkMiss}
          onOpenPasskey={openPasskey}
          hasPasskey={hasPasskey}
          theme={theme}
          onToggleTheme={toggleTheme}
        />
      )}
      {surface.kind === 'vault' && (
        <SpaceVault
          tree={tree}
          excerpts={excerpts}
          meta={meta}
          calendar={calendar}
          entries={railEntries}
          entriesLabel={railLabel}
          selectedDay={selectedDay}
          onClearSelectedDay={clearSelectedDay}
          onPickDate={pickDate}
          onSelectFile={onVaultSelectPath}
          onSelectDay={selectDay}
          onNewEntry={newEntry}
          onBackToReader={backFromVault}
          onDownloadFile={setDownloadFile}
          onRenameFile={handleRenameFile}
          onMoveFiles={handleMoveFiles}
          onCreateFolder={handleCreateFolder}
          onDeleteFiles={handleDeleteFiles}
          onSetTags={handleSetTags}
          onOpenPasskey={openPasskey}
          hasPasskey={hasPasskey}
          theme={theme}
          onToggleTheme={toggleTheme}
        />
      )}
      {surface.kind === 'preview' && (
        <Preview
          file={surface.file}
          calendar={calendar}
          entries={railEntries}
          entriesLabel={railLabel}
          selectedDay={selectedDay}
          onClearSelectedDay={clearSelectedDay}
          onPickDate={pickDate}
          onSelectFile={selectFile}
          onSelectDay={selectDay}
          onNewEntry={newEntry}
          onOpenVault={openVault}
          onLock={onLock}
          onDownload={setDownloadFile}
          onOpenPasskey={openPasskey}
          hasPasskey={hasPasskey}
          theme={theme}
          onToggleTheme={toggleTheme}
        />
      )}

      <SearchOverlay
        open={searchOpen}
        onClose={() => setSearchOpen(false)}
        onSelect={(path) => {
          void selectFile(path);
        }}
      />

      <UploadModal
        open={uploadOpen}
        initialFiles={uploadInitial}
        tree={tree}
        onClose={() => {
          setUploadOpen(false);
          setUploadInitial(undefined);
        }}
        onUploaded={() => {
          void onUploaded();
        }}
      />

      <DownloadModal
        open={downloadFile != null}
        file={downloadFile}
        onClose={() => setDownloadFile(null)}
      />

      <PasskeyModal
        open={passkeyOpen}
        email={view.email}
        owner={view.owner}
        hasPasskey={hasPasskey}
        onClose={() => setPasskeyOpen(false)}
        onChanged={() => {
          void refreshStatus();
        }}
      />

      {dragOverlay && (
        <div className="spaceDragOverlay">
          <div className="spaceDragOverlayInner">
            <div className="spaceDragOverlayIcon">↓</div>
            <div className="spaceDragOverlayTitle">Drop files anywhere</div>
            <div className="spaceDragOverlaySub">They'll be encrypted before they hit disk.</div>
          </div>
        </div>
      )}

      {toast && (
        <div className="spaceToast" role="status">
          {toast}
        </div>
      )}

      {!agentOpen && (
        <button
          type="button"
          className="spaceAgentFab"
          onClick={() => setAgentOpen(true)}
          title="Assistant"
          aria-label="Open the assistant"
        >
          <Sparkle size={22} />
        </button>
      )}

      <AgentChat
        open={agentOpen}
        onClose={() => setAgentOpen(false)}
        onVaultChanged={onAgentVaultChanged}
      />
    </div>
  );
}

/** Truncate a `Date` to the start of its calendar day, so the calendar/today
 *  memos only change across midnight rather than on every minute tick. */
function startOfDay(date: Date): Date {
  return new Date(date.getFullYear(), date.getMonth(), date.getDate());
}

function startOfMonth(date: Date): Date {
  return new Date(date.getFullYear(), date.getMonth(), 1);
}

function hasFiles(event: DragEvent): boolean {
  const types = event.dataTransfer?.types;
  if (!types) { return false; }
  for (const type of Array.from(types)) {
    if (type === 'Files') { return true; }
  }
  return false;
}

function buildTitleMap(tree: TreeNode[], excerpts: ExcerptMap): Map<string, string> {
  const out = new Map<string, string>();
  const walk = (nodes: TreeNode[]) => {
    for (const node of nodes) {
      if (node.type === 'file' && node.kind === 'md') {
        const title = excerpts[node.path]?.title ?? node.name.replace(/\.(md|markdown)$/i, '');
        if (title && !out.has(title)) { out.set(title, node.path); }
      } else if (node.type === 'folder') {
        walk(node.children);
      }
    }
  };
  walk(tree);
  return out;
}

function findInTree(tree: TreeNode[], path: string): TreeFile | null {
  const walk = (nodes: TreeNode[]): TreeFile | null => {
    for (const node of nodes) {
      if (node.type === 'file' && node.path === path) { return node; }
      if (node.type === 'folder') {
        const hit = walk(node.children);
        if (hit) { return hit; }
      }
    }
    return null;
  };
  return walk(tree);
}

function LoadingScreen() {
  return (
    <div
      style={{
        position: 'absolute',
        inset: 0,
        display: 'flex',
        alignItems: 'center',
        justifyContent: 'center',
        background: 'var(--paper)',
        color: 'var(--mute)',
        fontFamily: 'var(--font-serif)',
        fontStyle: 'italic',
        fontSize: 16,
      }}
    >
      Opening the door…
    </div>
  );
}

function FatalScreen({ message }: { message: string }) {
  return (
    <div
      style={{
        position: 'absolute',
        inset: 0,
        display: 'flex',
        alignItems: 'center',
        justifyContent: 'center',
        background: 'var(--paper)',
        color: 'var(--ink)',
        fontFamily: 'var(--font-serif)',
        padding: 32,
      }}
    >
      <div style={{ maxWidth: 480, textAlign: 'center' }}>
        <div
          style={{
            fontSize: 11,
            letterSpacing: '0.18em',
            textTransform: 'uppercase',
            color: 'var(--accent)',
            marginBottom: 14,
            fontWeight: 600,
          }}
        >
          A small problem
        </div>
        <div style={{ fontSize: 22, fontWeight: 500, letterSpacing: '-0.015em' }}>{message}</div>
      </div>
    </div>
  );
}
