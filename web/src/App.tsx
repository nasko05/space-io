import { DragEvent, useCallback, useEffect, useMemo, useRef, useState } from 'react';
import { Auth } from './components/Auth/Auth';
import { Reader } from './components/Reader/Reader';
import { HearthVault } from './components/Vault/HearthVault';
import { Preview } from './components/Preview/Preview';
import { SearchOverlay } from './components/Search/SearchOverlay';
import { UploadModal } from './components/Upload/UploadModal';
import { DownloadModal } from './components/Download/DownloadModal';
import { PasskeyModal } from './components/Passkey/PasskeyModal';
import {
  api,
  ExcerptMap,
  firstMarkdownLeaf,
  ReadFile,
  TreeFile,
  TreeNode,
} from './api/client';
import {
  buildCalendar,
  CalendarView,
  entriesForToday,
  findFileForDay,
  TodayEntry,
} from './lib/calendar';

type View =
  | { kind: 'loading' }
  | { kind: 'auth'; owner: string; hasPasskey: boolean }
  | { kind: 'unlocked'; owner: string; surface: Surface }
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
  const [now, setNow] = useState(() => new Date());
  const [hasPasskey, setHasPasskey] = useState(false);
  const [searchOpen, setSearchOpen] = useState(false);
  const [uploadOpen, setUploadOpen] = useState(false);
  const [uploadInitial, setUploadInitial] = useState<File[] | undefined>(undefined);
  const [downloadFile, setDownloadFile] = useState<TreeFile | null>(null);
  const [passkeyOpen, setPasskeyOpen] = useState(false);
  const [toast, setToast] = useState<string | null>(null);
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
    const t = window.setInterval(() => setNow(new Date()), 60_000);
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
    async (owner: string) => {
      try {
        const t = await refreshTree();
        void refreshExcerpts();
        const leaf = firstMarkdownLeaf(t);
        if (!leaf) {
          const { path } = await api.create(DEFAULT_NEW_FOLDER);
          await refreshTree();
          const file: ReadFile = { path, content: '', updated: null };
          previousPathRef.current = path;
          setView({
            kind: 'unlocked',
            owner,
            surface: { kind: 'reader', file, initialMode: 'edit' },
          });
          return;
        }
        const file = await api.read(leaf.path);
        previousPathRef.current = file.path;
        setView({
          kind: 'unlocked',
          owner,
          surface: { kind: 'reader', file, initialMode: 'preview' },
        });
      } catch (err) {
        setView({
          kind: 'fatal',
          message: err instanceof Error ? err.message : 'Failed to load the space.',
        });
      }
    },
    [refreshExcerpts, refreshTree],
  );

  useEffect(() => {
    let cancelled = false;
    (async () => {
      try {
        const status = await api.status();
        if (cancelled) return;
        setHasPasskey(status.has_passkey);
        if (status.unlocked) {
          await enterReader(status.owner);
        } else {
          setView({ kind: 'auth', owner: status.owner, hasPasskey: status.has_passkey });
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

  const onUnlocked = useCallback(() => {
    if (view.kind !== 'auth') return;
    setView({ kind: 'loading' });
    void enterReader(view.owner);
  }, [enterReader, view]);

  const onLock = useCallback(async () => {
    try {
      await api.lock();
    } catch {
      // ignore
    }
    setTree([]);
    setExcerpts({});
    previousPathRef.current = null;
    setSearchOpen(false);
    setUploadOpen(false);
    setDownloadFile(null);
    setPasskeyOpen(false);
    const status = await refreshStatus();
    setView({
      kind: 'auth',
      owner: status?.owner ?? (view.kind === 'unlocked' ? view.owner : 'ada@home.lan'),
      hasPasskey: status?.has_passkey ?? hasPasskey,
    });
  }, [hasPasskey, refreshStatus, view]);

  const selectFile = useCallback(
    async (path: string) => {
      if (view.kind !== 'unlocked') return;
      try {
        const file = await api.read(path);
        previousPathRef.current = path;
        setView({
          kind: 'unlocked',
          owner: view.owner,
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
      surface: { kind: 'vault', previousPath: previousPathRef.current },
    });
    try {
      await Promise.all([refreshTree(), refreshExcerpts()]);
    } catch (err) {
      console.error('failed to refresh vault', err);
    }
  }, [refreshExcerpts, refreshTree, view]);

  const backFromVault = useCallback(async () => {
    if (view.kind !== 'unlocked') return;
    if (view.surface.kind !== 'vault') return;
    const target = view.surface.previousPath;
    if (!target) {
      void enterReader(view.owner);
      return;
    }
    try {
      const file = await api.read(target);
      setView({
        kind: 'unlocked',
        owner: view.owner,
        surface: { kind: 'reader', file, initialMode: 'preview' },
      });
    } catch {
      void enterReader(view.owner);
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
        surface: { kind: 'reader', file, initialMode: 'edit' },
      });
    } catch (err) {
      console.error('failed to create new entry', err);
      setToast(err instanceof Error ? err.message : 'Could not create new entry');
    }
  }, [refreshExcerpts, refreshTree, view]);

  const saveFile = useCallback(
    async (path: string, content: string) => {
      await api.write(path, content);
      // Locally patch the title/excerpt so wikilink autocomplete + the Today
      // list reflect the latest content without a full server walk-and-
      // decrypt on every keystroke (the prior implementation made the UI
      // very slow as the corpus grew).
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
    },
    [],
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

  const selectDay = useCallback(
    (day: number) => {
      if (view.kind !== 'unlocked') return;
      const target = findFileForDay(tree, now.getFullYear(), now.getMonth(), day);
      if (target) void selectFile(target.path);
    },
    [now, selectFile, tree, view],
  );

  const onUploaded = useCallback(async () => {
    await refreshTree();
    void refreshExcerpts();
  }, [refreshExcerpts, refreshTree]);

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

  const calendar: CalendarView = useMemo(() => buildCalendar(now, tree), [now, tree]);
  const currentPath =
    view.kind === 'unlocked' && view.surface.kind === 'reader' ? view.surface.file.path : null;
  const today: TodayEntry[] = useMemo(
    () => entriesForToday(now, tree, excerpts, currentPath),
    [now, tree, excerpts, currentPath],
  );
  const titleToPath = useMemo(() => buildTitleMap(tree, excerpts), [tree, excerpts]);

  if (view.kind === 'loading') return <LoadingScreen />;
  if (view.kind === 'auth')
    return <Auth owner={view.owner} hasPasskey={view.hasPasskey} onUnlocked={onUnlocked} />;
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
          today={today}
          titleToPath={titleToPath}
          onSelectFile={selectFile}
          onSelectDay={selectDay}
          onNewEntry={newEntry}
          onOpenVault={openVault}
          onOpenSearch={() => setSearchOpen(true)}
          onLock={onLock}
          onSave={saveFile}
          onWikilinkMiss={onWikilinkMiss}
          onOpenPasskey={() => setPasskeyOpen(true)}
          hasPasskey={hasPasskey}
          theme={theme}
          onToggleTheme={toggleTheme}
        />
      )}
      {surface.kind === 'vault' && (
        <HearthVault
          tree={tree}
          excerpts={excerpts}
          calendar={calendar}
          today={today}
          onSelectFile={(p) => {
            const file = findInTree(tree, p);
            if (file) onSelectVaultFile(file);
            else void selectFile(p);
          }}
          onSelectDay={selectDay}
          onNewEntry={newEntry}
          onBackToReader={backFromVault}
          onOpenPasskey={() => setPasskeyOpen(true)}
          hasPasskey={hasPasskey}
          theme={theme}
          onToggleTheme={toggleTheme}
        />
      )}
      {surface.kind === 'preview' && (
        <Preview
          file={surface.file}
          calendar={calendar}
          today={today}
          onSelectFile={selectFile}
          onSelectDay={selectDay}
          onNewEntry={newEntry}
          onOpenVault={openVault}
          onLock={onLock}
          onDownload={(f) => setDownloadFile(f)}
          onOpenPasskey={() => setPasskeyOpen(true)}
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
    </div>
  );
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
