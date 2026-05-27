import { useCallback, useEffect, useState } from 'react';
import { Auth } from './components/Auth/Auth';
import { Reader } from './components/Reader/Reader';
import { api, firstMarkdownLeaf, ReadFile } from './api/client';

type View =
  | { kind: 'loading' }
  | { kind: 'auth'; owner: string }
  | { kind: 'reader'; file: ReadFile; owner: string }
  | { kind: 'fatal'; message: string };

export function App() {
  const [view, setView] = useState<View>({ kind: 'loading' });

  const enterReader = useCallback(async (owner: string) => {
    try {
      const { tree } = await api.tree();
      const leaf = firstMarkdownLeaf(tree);
      if (!leaf) {
        setView({
          kind: 'fatal',
          message: 'No markdown notes found in this space.',
        });
        return;
      }
      const file = await api.read(leaf.path);
      setView({ kind: 'reader', file, owner });
    } catch (err) {
      setView({
        kind: 'fatal',
        message: err instanceof Error ? err.message : 'Failed to load the space.',
      });
    }
  }, []);

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
    setView((v) => ({ kind: 'auth', owner: v.kind === 'reader' ? v.owner : 'ada@home.lan' }));
  }, []);

  if (view.kind === 'loading') {
    return <LoadingScreen />;
  }
  if (view.kind === 'auth') {
    return <Auth owner={view.owner} onUnlocked={onUnlocked} />;
  }
  if (view.kind === 'reader') {
    return (
      <Reader
        path={view.file.path}
        content={view.file.content}
        updated={view.file.updated}
        onLock={onLock}
      />
    );
  }
  return <FatalScreen message={view.message} />;
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
