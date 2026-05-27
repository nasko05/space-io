import { useCallback, useEffect, useMemo, useRef, useState } from 'react';
import { Auth } from './components/Auth/Auth';
import { Reader } from './components/Reader/Reader';
import { HearthVault } from './components/Vault/HearthVault';
import {
  api,
  ExcerptMap,
  firstMarkdownLeaf,
  ReadFile,
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
  | { kind: 'auth'; owner: string }
  | { kind: 'unlocked'; owner: string; surface: Surface }
  | { kind: 'fatal'; message: string };

type Surface =
  | { kind: 'reader'; file: ReadFile; initialMode: 'preview' | 'edit' }
  | { kind: 'vault'; previousPath: string | null };

const DEFAULT_NEW_FOLDER = (() => {
  const year = new Date().getFullYear();
  return `Journal/${year}`;
})();

export function App() {
  const [view, setView] = useState<View>({ kind: 'loading' });
  const [tree, setTree] = useState<TreeNode[]>([]);
  const [excerpts, setExcerpts] = useState<ExcerptMap>({});
  const [now, setNow] = useState(() => new Date());

  // Refresh "now" once a minute so the rail's date can rollover.
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
    const { excerpts } = await api.excerpts();
    setExcerpts(excerpts);
  }, []);

  const enterReader = useCallback(
    async (owner: string) => {
      try {
        const t = await refreshTree();
        const leaf = firstMarkdownLeaf(t);
        if (!leaf) {
          // No notes yet — drop into a fresh draft.
          const { path } = await api.create(DEFAULT_NEW_FOLDER);
          const t2 = await refreshTree();
          const file: ReadFile = { path, content: '', updated: null };
          previousPathRef.current = path;
          setView({
            kind: 'unlocked',
            owner,
            surface: { kind: 'reader', file, initialMode: 'edit' },
          });
          void t2;
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
    [refreshTree],
  );

  // Bootstrap.
  useEffect(() => {
    let cancelled = false;
    (async () => {
      try {
        const status = await api.status();
        if (cancelled) return;
        if (status.unlocked) {
          await enterReader(status.owner);
        } else {
          setView({ kind: 'auth', owner: status.owner });
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
      // ignore — we're locking either way
    }
    setTree([]);
    setExcerpts({});
    previousPathRef.current = null;
    setView((v) =>
      v.kind === 'unlocked'
        ? { kind: 'auth', owner: v.owner }
        : { kind: 'auth', owner: 'ada@home.lan' },
    );
  }, []);

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
      } catch (err) {
        // Non-fatal: surface in console for now; could become a toast later.
        console.error('failed to read file', err);
      }
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
      previousPathRef.current = path;
      setView({
        kind: 'unlocked',
        owner: view.owner,
        surface: { kind: 'reader', file, initialMode: 'edit' },
      });
    } catch (err) {
      console.error('failed to create new entry', err);
    }
  }, [refreshTree, view]);

  const saveFile = useCallback(
    async (path: string, content: string) => {
      await api.write(path, content);
    },
    [],
  );

  const selectDay = useCallback(
    (day: number) => {
      if (view.kind !== 'unlocked') return;
      const target = findFileForDay(tree, now.getFullYear(), now.getMonth(), day);
      if (target) {
        void selectFile(target.path);
      }
    },
    [now, selectFile, tree, view],
  );

  const calendar: CalendarView = useMemo(() => buildCalendar(now, tree), [now, tree]);
  const currentPath =
    view.kind === 'unlocked' && view.surface.kind === 'reader' ? view.surface.file.path : null;
  const today: TodayEntry[] = useMemo(
    () => entriesForToday(now, tree, excerpts, currentPath),
    [now, tree, excerpts, currentPath],
  );

  if (view.kind === 'loading') return <LoadingScreen />;
  if (view.kind === 'auth') return <Auth owner={view.owner} onUnlocked={onUnlocked} />;
  if (view.kind === 'fatal') return <FatalScreen message={view.message} />;

  const { surface } = view;
  if (surface.kind === 'reader') {
    return (
      <Reader
        path={surface.file.path}
        content={surface.file.content}
        updated={surface.file.updated}
        initialMode={surface.initialMode}
        calendar={calendar}
        today={today}
        onSelectFile={selectFile}
        onSelectDay={selectDay}
        onNewEntry={newEntry}
        onOpenVault={openVault}
        onLock={onLock}
        onSave={saveFile}
      />
    );
  }
  return (
    <HearthVault
      tree={tree}
      excerpts={excerpts}
      calendar={calendar}
      today={today}
      onSelectFile={(p) => {
        void selectFile(p);
      }}
      onSelectDay={selectDay}
      onNewEntry={newEntry}
      onBackToReader={backFromVault}
    />
  );
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
