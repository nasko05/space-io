import { DragEvent, useCallback, useEffect, useMemo, useRef, useState } from 'react';
import { Auth } from './components/Auth/Auth';
import { Registration } from './components/Registration/Registration';
import { Reader } from './components/Reader/Reader';
import { HearthVault } from './components/Vault/HearthVault';
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
  // The calendar and the entries list only care about which **day** it is,
  // so we track a day-stable date that flips only across midnight. Cards
  // are memoized on `file.updated`, so they don't need a minute-rate tick
  // to stay accurate — the previous `now` state was unread machinery.
  const [todayDate, setTodayDate] = useState(() => startOfDay(new Date()));
  const [hasPasskey, setHasPasskey] = useState(false);
  const [searchOpen, setSearchOpen] = useState(false);
  const [uploadOpen, setUploadOpen] = useState(false);
  const [uploadInitial, setUploadInitial] = useState<File[] | undefined>(undefined);
  const [downloadFile, setDownloadFile] = useState<TreeFile | null>(null);
  const [passkeyOpen, setPasskeyOpen] = useState(false);
  const [agentOpen, setAgentOpen] = useState(false);
  const [toast, setToast] = useState<string | null>(null);
  // Calendar selection. `null` means "show today" (the default); a number
  // pins the rail's entry list to that day-of-month in the displayed
  // calendar. Clicking the same day again clears the selection.
  const [selectedDay, setSelectedDay] = useState<number | null>(null);
  const [theme, setTheme] = useState<'light' | 'dark'>(() => {
    const stored = typeof window !== 'undefined' ? window.localStorage.getItem('hearth.theme') : null;
    return stored === 'dark' ? 'dark' : 'light';
  });

  useEffect(() => {
    if (typeof document === 'undefined') return;
    document.documentElement.dataset.theme = theme;
    window.localStorage.setItem('hearth.theme', theme);
  }, [theme]);

  const toggleTheme = useCallback(() => {
    setTheme((t) => (t === 'dark' ? 'light' : 'dark'));
  }, []);

  useEffect(() => {
    const tick = () => {
      const next = startOfDay(new Date());
      setTodayDate((prev) => (prev.getTime() === next.getTime() ? prev : next));
    };
    const t = window.setInterval(tick, 60_000);
    return () => window.clearInterval(t);
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
        const t = await refreshTree();
        void refreshExcerpts();
        void refreshMeta();
        const leaf = firstMarkdownLeaf(t);
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
        if (cancelled) return;
        setHasPasskey(status.has_passkey);
        if (status.unlocked) {
          await enterReader(status.owner, status.email);
        } else if (!status.any_users) {
          setView({ kind: 'registration', anyUsers: false });
        } else {
          setView({ kind: 'auth', anyUsers: true });
        }
      } catch (err) {
        if (cancelled) return;
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
    // `/api/auth/init` mints the session cookie itself, so we just need to
    // re-fetch status (to learn the chosen display name) and drop into the
    // reader. The seed welcome note is already in place under the new UUID
    // folder.
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
    if (view.kind !== 'auth') return;
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

  const onLock = useCallback(async () => {
    try {
      await api.lock();
    } catch {
      // ignore
    }
    setTree([]);
    setExcerpts({});
    setMeta({});
    previousPathRef.current = null;
    // Reset every overlay/modal piece so reopening the app after a lock
    // never starts inside a half-open dialog or with a stale upload queue
    // pinned to a now-defunct session.
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
      if (view.kind !== 'unlocked') return;
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
      if (view.kind !== 'unlocked') return;
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
    if (view.kind !== 'unlocked') return;
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
    if (view.kind !== 'unlocked') return;
    if (view.surface.kind !== 'vault') return;
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
    if (view.kind !== 'unlocked') return;
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

  // Recompute the local title/excerpt for `path` so wikilink autocomplete +
  // the Today list reflect the latest content without a full server walk-and-
  // decrypt on every keystroke (the prior implementation made the UI very
  // slow as the corpus grew). Shared by autosave and checkpoint.
  const patchExcerpt = useCallback((path: string, content: string) => {
    const titleMatch = /^# (.+)$/m.exec(content);
    const title = titleMatch ? titleMatch[1].trim() : null;
    const bodyLines = content
      .split('\n')
      .filter((l) => !l.startsWith('#') && l.trim().length > 0)
      .slice(0, 3)
      .join(' ');
    const excerpt = bodyLines
      .replace(/[*_`]/g, '')
      .replace(/\[\[|\]\]/g, '')
      .slice(0, 180);
    setExcerpts((cur) => ({ ...cur, [path]: { title, excerpt } }));
  }, []);

  // Autosave: persist the draft to disk. Does NOT create a history entry.
  const saveFile = useCallback(
    async (path: string, content: string) => {
      await api.saveDraft(path, content);
      patchExcerpt(path, content);
    },
    [patchExcerpt],
  );

  // Checkpoint: persist + record a labelled point in the version history.
  const checkpointFile = useCallback(
    async (path: string, content: string, message?: string) => {
      await api.checkpoint(path, content, message);
      patchExcerpt(path, content);
    },
    [patchExcerpt],
  );

  const rollbackFile = useCallback(
    async (path: string, commit: string) => {
      if (view.kind !== 'unlocked') return;
      await api.rollback(path, commit);
      // Server wrote a new commit on top of HEAD with the old content. Pull
      // the fresh file + refresh the tree so the rail / Today list reflect
      // the restored excerpt, then re-open the file in the Reader so the
      // editor's local state isn't stuck on the pre-rollback content.
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

  // Resolve a path to a `TreeFile` (for preview routing) and select. Wrapped
  // here so HearthVault gets a stable handler reference, which keeps the
  // memoized rail from re-rendering on unrelated parent updates.
  const onVaultSelectPath = useCallback(
    (p: string) => {
      const file = findInTree(tree, p);
      if (file) onSelectVaultFile(file);
      else void selectFile(p);
    },
    [onSelectVaultFile, selectFile, tree],
  );

  const openSearch = useCallback(() => setSearchOpen(true), []);
  const openPasskey = useCallback(() => setPasskeyOpen(true), []);

  // After the assistant applies an approved change, pull fresh tree/excerpts/
  // tags so the rail, calendar, and vault reflect it immediately.
  const onAgentVaultChanged = useCallback(() => {
    void refreshTree();
    void refreshExcerpts();
    void refreshMeta();
  }, [refreshExcerpts, refreshMeta, refreshTree]);

  const onUploaded = useCallback(async () => {
    await refreshTree();
    void refreshExcerpts();
  }, [refreshExcerpts, refreshTree]);

  // ---- Vault CRUD handlers (Phase 4) ----

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
          .filter((m) => m.from !== m.to);
        if (moves.length === 0) return;
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
      if (paths.length === 0) return;
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
      if (paths.length === 0) return;
      try {
        await api.setTagsBulk(paths.map((path) => ({ path, tags })));
        // Patch local state immediately so the UI doesn't blink while the
        // server walk catches up.
        setMeta((cur) => {
          const next = { ...cur };
          for (const p of paths) {
            if (tags.length === 0) delete next[p];
            else next[p] = { tags };
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
    function onKey(e: KeyboardEvent) {
      if ((e.metaKey || e.ctrlKey) && e.key.toLowerCase() === 'k') {
        if (view.kind === 'unlocked') {
          e.preventDefault();
          setSearchOpen((v) => !v);
        }
      } else if (e.key === 'Escape') {
        if (searchOpen) setSearchOpen(false);
      }
    }
    window.addEventListener('keydown', onKey);
    return () => window.removeEventListener('keydown', onKey);
  }, [searchOpen, view]);

  const dragCounter = useRef(0);
  const [dragOverlay, setDragOverlay] = useState(false);
  const handleWindowDragEnter = useCallback(
    (e: DragEvent) => {
      if (view.kind !== 'unlocked') return;
      if (!hasFiles(e)) return;
      e.preventDefault();
      dragCounter.current += 1;
      setDragOverlay(true);
    },
    [view],
  );
  const handleWindowDragLeave = useCallback((e: DragEvent) => {
    e.preventDefault();
    dragCounter.current -= 1;
    if (dragCounter.current <= 0) {
      dragCounter.current = 0;
      setDragOverlay(false);
    }
  }, []);
  const handleWindowDrop = useCallback(
    (e: DragEvent) => {
      if (view.kind !== 'unlocked') return;
      if (!hasFiles(e)) return;
      e.preventDefault();
      dragCounter.current = 0;
      setDragOverlay(false);
      const files = Array.from(e.dataTransfer?.files ?? []);
      if (files.length === 0) return;
      setUploadInitial(files);
      setUploadOpen(true);
    },
    [view],
  );

  useEffect(() => {
    if (!toast) return;
    const t = window.setTimeout(() => setToast(null), 3200);
    return () => window.clearTimeout(t);
  }, [toast]);

  // Memoize on `todayDate` (advances only across midnight) so the calendar
  // doesn't rebuild on every minute tick. `buildCalendar` and `entriesForDate`
  // both treat their first arg as a day-precision date — they don't read
  // hours/minutes — so a day-stable input is exactly what they need.
  const calendar: CalendarView = useMemo(
    () => buildCalendar(todayDate, tree),
    [todayDate, tree],
  );
  // If the calendar slides into a new month while a day is pinned (e.g. a
  // long-running session ticks past midnight on the last of the month),
  // drop the selection so we don't keep highlighting a day that no longer
  // exists in the displayed grid.
  useEffect(() => {
    if (selectedDay != null && selectedDay > calendar.daysInMonth) {
      setSelectedDay(null);
    }
  }, [calendar.daysInMonth, selectedDay]);

  const currentPath =
    view.kind === 'unlocked' && view.surface.kind === 'reader' ? view.surface.file.path : null;
  const railEntries: TodayEntry[] = useMemo(() => {
    const target =
      selectedDay != null ? new Date(calendar.year, calendar.month, selectedDay) : todayDate;
    return entriesForDate(target, tree, excerpts, currentPath);
  }, [todayDate, selectedDay, calendar.year, calendar.month, tree, excerpts, currentPath]);
  const railLabel: string =
    selectedDay != null ? shortDayLabel(calendar.month, selectedDay) : 'Today';
  const titleToPath = useMemo(() => buildTitleMap(tree, excerpts), [tree, excerpts]);

  if (view.kind === 'loading') return <LoadingScreen />;
  if (view.kind === 'registration')
    return (
      <Registration
        showBackToLogin={view.anyUsers}
        onRegistered={onRegistered}
        onBackToLogin={showLogin}
      />
    );
  if (view.kind === 'auth')
    return (
      <Auth
        showRegisterLink={view.anyUsers}
        onUnlocked={onUnlocked}
        onRegister={showRegistration}
      />
    );
  if (view.kind === 'fatal') return <FatalScreen message={view.message} />;

  const { surface } = view;
  return (
    <div
      onDragEnter={handleWindowDragEnter}
      onDragOver={(e) => {
        if (hasFiles(e)) e.preventDefault();
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
        <HearthVault
          tree={tree}
          excerpts={excerpts}
          meta={meta}
          calendar={calendar}
          entries={railEntries}
          entriesLabel={railLabel}
          selectedDay={selectedDay}
          onClearSelectedDay={clearSelectedDay}
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
        onSelect={(p) => {
          void selectFile(p);
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
        <div className="hearthDragOverlay">
          <div className="hearthDragOverlayInner">
            <div className="hearthDragOverlayIcon">↓</div>
            <div className="hearthDragOverlayTitle">Drop files anywhere</div>
            <div className="hearthDragOverlaySub">They'll be encrypted before they hit disk.</div>
          </div>
        </div>
      )}

      {toast && (
        <div className="hearthToast" role="status">
          {toast}
        </div>
      )}

      {!agentOpen && (
        <button
          type="button"
          className="hearthAgentFab"
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

/** Truncate a `Date` to the start of its calendar day in local time. Used to
 *  give the calendar/today memos a value that only changes when the day
 *  actually crosses midnight, not on every minute tick. */
function startOfDay(d: Date): Date {
  return new Date(d.getFullYear(), d.getMonth(), d.getDate());
}

function hasFiles(e: DragEvent): boolean {
  const types = e.dataTransfer?.types;
  if (!types) return false;
  for (const t of Array.from(types)) {
    if (t === 'Files') return true;
  }
  return false;
}

function buildTitleMap(tree: TreeNode[], excerpts: ExcerptMap): Map<string, string> {
  const out = new Map<string, string>();
  const walk = (nodes: TreeNode[]) => {
    for (const n of nodes) {
      if (n.type === 'file' && n.kind === 'md') {
        const title = excerpts[n.path]?.title ?? n.name.replace(/\.(md|markdown)$/i, '');
        if (title && !out.has(title)) out.set(title, n.path);
      } else if (n.type === 'folder') {
        walk(n.children);
      }
    }
  };
  walk(tree);
  return out;
}

function findInTree(tree: TreeNode[], path: string): TreeFile | null {
  const walk = (nodes: TreeNode[]): TreeFile | null => {
    for (const n of nodes) {
      if (n.type === 'file' && n.path === path) return n;
      if (n.type === 'folder') {
        const hit = walk(n.children);
        if (hit) return hit;
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
