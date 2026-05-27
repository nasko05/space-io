import { HearthShell } from '../Shell/HearthShell';
import { HearthRail } from '../Rail/HearthRail';
import { Markdown } from '../Markdown/Markdown';
import { Chevron, Clock, Eye, Folder, Link, Pencil, Search, Tag } from '../icons/Icon';
import { extractTitle, stripFirstH1 } from '../../lib/markdown';
import styles from './Reader.module.css';

interface Props {
  path: string;
  content: string;
  updated: string | null;
  onLock: () => void;
}

// Ported from dir-1-hearth.jsx:156-252 (HearthMain).
// Slice: tree click navigation, Edit toggle, search, and Linked-from are
// visual-only or deferred.
export function Reader({ path, content, updated, onLock }: Props) {
  const segments = path.split('/');
  const fileName = segments[segments.length - 1] ?? path;
  const folderSegments = segments.slice(0, -1);

  const titleFromContent = extractTitle(content);
  const titleParts = splitTitle(titleFromContent ?? fileNameToTitle(fileName));
  const bodySource = stripFirstH1(content);
  const wordCount = countWords(content);
  const readMin = Math.max(1, Math.round(wordCount / 220));

  return (
    <HearthShell mode="reading" onLock={onLock}>
      <div className={styles.layout}>
        <HearthRail />
        <main className={styles.main}>
          <div className={styles.toolbar}>
            <div className={styles.crumb}>
              <Folder size={13} />
              {folderSegments.map((seg, i) => (
                <span key={i} className={styles.crumbSeg}>
                  {seg}
                  <Chevron size={10} />
                </span>
              ))}
              <span className={styles.crumbFile}>{fileName}</span>
            </div>
            <div className={styles.toolbarRight}>
              <span className={styles.meta}>
                <Clock size={12} /> {readMin} min read
              </span>
              <span className={styles.meta}>{wordCount} words</span>
              <button type="button" className={styles.toolBtn}>
                <Eye size={12} /> Preview
              </button>
              <button type="button" className={`${styles.toolBtn} ${styles.toolBtnActive}`}>
                <Pencil size={12} /> Edit
              </button>
            </div>
          </div>

          <div className={styles.searchPill} aria-hidden>
            <Search size={12} />
            <span>Search the whole diary…</span>
            <kbd>⌘K</kbd>
          </div>

          <div className={styles.column}>
            <article className={styles.article}>
              <div className={styles.dateline}>
                <span>{formatDateline(updated)}</span>
                <span className={styles.datelineRule} />
                <span className={styles.tags}>
                  <Tag size={11} /> journal · morning-pages
                </span>
              </div>

              <h1 className={styles.title}>
                {titleParts[0]}
                {titleParts.length > 1 && (
                  <>
                    ,<br />
                    <em>{titleParts.slice(1).join(', ')}</em>
                  </>
                )}
              </h1>

              <Markdown source={bodySource} />

              <div className={styles.linkedFrom}>
                <Link size={12} />
                <span className={styles.linkedLabel}>Linked from</span>
                <a className={styles.linkedLink}>On memory palaces</a>
                <a className={styles.linkedLink}>Notes from M.</a>
              </div>
            </article>
          </div>
        </main>
      </div>
    </HearthShell>
  );
}

function splitTitle(title: string): string[] {
  const parts = title.split(',').map((p) => p.trim());
  return parts.length > 1 ? parts : [title];
}

function fileNameToTitle(name: string): string {
  return name.replace(/\.(md|markdown)$/i, '');
}

function countWords(src: string): number {
  return src
    .split(/\s+/)
    .filter((w) => /\w/.test(w))
    .length;
}

function formatDateline(updated: string | null): string {
  const d = updated ? new Date(updated) : new Date();
  const weekday = d.toLocaleDateString(undefined, { weekday: 'long' });
  const day = d.toLocaleDateString(undefined, { day: 'numeric', month: 'long', year: 'numeric' });
  const time = d.toLocaleTimeString(undefined, { hour: '2-digit', minute: '2-digit', hour12: false });
  return `${weekday} · ${day} · ${time}`;
}
