import { lazy, Suspense, useEffect, useState } from 'react';
import { SpaceShell } from '../Shell/SpaceShell';
import { SpaceRail } from '../Rail/SpaceRail';
import { Chevron, Download as DownloadIcon, Folder } from '../icons/Icon';
import { api, TreeFile } from '../../api/client';
import { CalendarView, TodayEntry } from '../../lib/calendar';
import { formatSize } from '../../lib/format';
import styles from './Preview.module.css';

const PdfRenderer = lazy(() => import('./PdfRenderer'));
const DocxRenderer = lazy(() => import('./DocxRenderer'));

interface Props {
  file: TreeFile;
  calendar: CalendarView;
  entries: TodayEntry[];
  entriesLabel: string;
  selectedDay: number | null;
  onClearSelectedDay: () => void;
  onPickDate: (value: string) => void;
  onSelectFile: (path: string) => void;
  onSelectDay: (day: number) => void;
  onNewEntry: () => void;
  onOpenVault: () => void;
  onLock: () => void;
  onDownload: (file: TreeFile) => void;
  onOpenPasskey?: () => void;
  hasPasskey?: boolean;
  theme?: 'light' | 'dark';
  onToggleTheme?: () => void;
}

type FetchState =
  | { kind: 'loading' }
  | { kind: 'ready'; data: ArrayBuffer }
  | { kind: 'error'; message: string };

export function Preview({
  file,
  calendar,
  entries,
  entriesLabel,
  selectedDay,
  onClearSelectedDay,
  onPickDate,
  onSelectFile,
  onSelectDay,
  onNewEntry,
  onOpenVault,
  onLock,
  onDownload,
  onOpenPasskey,
  hasPasskey,
  theme,
  onToggleTheme,
}: Props) {
  const needsBinary = file.kind === 'pdf' || file.kind === 'docx';
  const [state, setState] = useState<FetchState>({ kind: 'loading' });

  useEffect(() => {
    if (!needsBinary) {
      setState({ kind: 'ready', data: new ArrayBuffer(0) });
      return;
    }
    let cancelled = false;
    setState({ kind: 'loading' });
    (async () => {
      try {
        const res = await fetch(api.downloadUrl(file.path), {
          credentials: 'same-origin',
        });
        if (!res.ok) { throw new Error(`fetch failed (${res.status})`); }
        const data = await res.arrayBuffer();
        if (!cancelled) { setState({ kind: 'ready', data }); }
      } catch (err) {
        if (!cancelled) {
          setState({
            kind: 'error',
            message: err instanceof Error ? err.message : 'fetch failed',
          });
        }
      }
    })();
    return () => {
      cancelled = true;
    };
  }, [file.path, needsBinary]);

  const segments = file.path.split('/');
  const fileName = segments[segments.length - 1] ?? file.path;
  const folderSegments = segments.slice(0, -1);

  return (
    <SpaceShell
      mode={kindLabel(file.kind)}
      onLock={onLock}
      theme={theme}
      onToggleTheme={onToggleTheme}
    >
      <div className={styles.layout}>
        <SpaceRail
          calendar={calendar}
          entries={entries}
          entriesLabel={entriesLabel}
          selectedDay={selectedDay}
          onClearSelectedDay={onClearSelectedDay}
          onPickDate={onPickDate}
          onNewEntry={onNewEntry}
          onSelectFile={onSelectFile}
          onSelectDay={onSelectDay}
          onOpenVault={onOpenVault}
          onOpenPasskey={onOpenPasskey}
          hasPasskey={hasPasskey}
          activeSurface="reader"
        />
        <main className={styles.main}>
          <div className={styles.toolbar}>
            <div className={styles.crumb}>
              <Folder size={13} />
              {folderSegments.map((segment, i) => (
                <span key={i} className={styles.crumbSeg}>
                  {segment}
                  <Chevron size={10} />
                </span>
              ))}
              <span className={styles.crumbFile}>{fileName}</span>
            </div>
            <div className={styles.toolbarRight}>
              <span className={styles.meta}>{formatSize(file.size)}</span>
              <button type="button" className={styles.toolBtn} onClick={() => onDownload(file)}>
                <DownloadIcon size={12} /> Save to disk
              </button>
            </div>
          </div>

          <div className={styles.surface}>
            {state.kind === 'loading' && (
              <div className={styles.center}>
                <div className={styles.spinner}>Decrypting…</div>
              </div>
            )}
            {state.kind === 'error' && (
              <div className={styles.center}>
                <div className={styles.errorBlock}>
                  <div className={styles.errorTitle}>Couldn't open this file</div>
                  <div className={styles.errorBody}>{state.message}</div>
                </div>
              </div>
            )}
            {state.kind === 'ready' && (
              <Suspense
                fallback={
                  <div className={styles.center}>
                    <div className={styles.spinner}>Loading renderer…</div>
                  </div>
                }
              >
                <Renderer file={file} data={state.data} />
              </Suspense>
            )}
          </div>
        </main>
      </div>
    </SpaceShell>
  );
}

function Renderer({ file, data }: { file: TreeFile; data: ArrayBuffer }) {
  if (file.kind === 'pdf') { return <PdfRenderer data={data} />; }
  if (file.kind === 'docx') { return <DocxRenderer data={data} />; }
  if (file.kind === 'image') {
    return (
      <div className={styles.mediaCenter}>
        <img className={styles.image} src={api.downloadUrl(file.path)} alt={file.name} />
      </div>
    );
  }
  if (file.kind === 'video') {
    return (
      <div className={styles.mediaCenter}>
        <video className={styles.video} src={api.downloadUrl(file.path)} controls preload="metadata" />
      </div>
    );
  }
  return (
    <div className={styles.center}>
      <div className={styles.errorBlock}>
        <div className={styles.errorTitle}>No preview available</div>
        <div className={styles.errorBody}>
          Save this file to disk to open it locally.
        </div>
      </div>
    </div>
  );
}

function kindLabel(kind: string): string {
  if (kind === 'pdf') { return 'reading PDF'; }
  if (kind === 'docx') { return 'reading DOC'; }
  if (kind === 'image') { return 'viewing'; }
  if (kind === 'video') { return 'watching'; }
  return 'previewing';
}

