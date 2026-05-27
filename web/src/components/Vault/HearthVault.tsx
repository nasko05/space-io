import { WindowChrome } from '../WindowChrome/WindowChrome';
import { HearthRail } from '../Rail/HearthRail';
import { HearthCard } from './HearthCard';
import { ExcerptMap, TreeFile, TreeFolder, TreeNode } from '../../api/client';
import { CalendarView, TodayEntry } from '../../lib/calendar';
import styles from './HearthVault.module.css';

interface Props {
  tree: TreeNode[];
  excerpts: ExcerptMap;
  calendar: CalendarView;
  today: TodayEntry[];
  onSelectFile: (path: string) => void;
  onSelectDay: (day: number) => void;
  onNewEntry: () => void;
  onBackToReader: () => void;
  onOpenPasskey?: () => void;
  hasPasskey?: boolean;
}

// Ported from dir-1-hearth.jsx:670-765 (HearthVault).
// Phase 2: Shelves view only, .md cards only. Timeline/Grid views and
// PDF/DOCX/media cards are deferred — they need uploads to be useful.
export function HearthVault({
  tree,
  excerpts,
  calendar,
  today,
  onSelectFile,
  onSelectDay,
  onNewEntry,
  onBackToReader,
  onOpenPasskey,
  hasPasskey,
}: Props) {
  const folders: TreeFolder[] = tree.filter((n): n is TreeFolder => n.type === 'folder');
  const totalFiles = countFiles(tree);

  return (
    <div className={styles.root}>
      <WindowChrome
        title="SpaceIO · my space"
        right={<span className={styles.chromeCount}>{totalFiles} items</span>}
      />
      <div className={styles.layout}>
        <HearthRail
          calendar={calendar}
          today={today}
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
              <button type="button" className={`${styles.viewBtn} ${styles.viewBtnActive}`}>
                Shelves
              </button>
              <button type="button" className={styles.viewBtn} disabled title="coming soon">
                Timeline
              </button>
              <button type="button" className={styles.viewBtn} disabled title="coming soon">
                Grid
              </button>
            </div>
          </header>

          <div className={styles.shelves}>
            {folders.length === 0 && (
              <div className={styles.empty}>
                Your space is empty. Press <em>New entry</em> to write your first note.
              </div>
            )}
            {folders.map((folder, si) => {
              const files = collectFilesUnder(folder);
              const visible = files.slice(0, 8);
              return (
                <section key={folder.path} className={styles.shelf}>
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
                  </div>

                  <div className={styles.grid}>
                    {visible.map((file) => (
                      <HearthCard
                        key={file.path}
                        file={file}
                        excerpt={excerpts[file.path]}
                        onOpen={() => onSelectFile(file.path)}
                      />
                    ))}
                  </div>
                </section>
              );
            })}
          </div>
        </main>
      </div>
    </div>
  );
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
