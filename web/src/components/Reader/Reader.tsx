import { ChangeEvent, useEffect, useMemo, useRef, useState } from 'react';
import { HearthShell } from '../Shell/HearthShell';
import { HearthRail } from '../Rail/HearthRail';
import { Markdown } from '../Markdown/Markdown';
import { Chevron, Clock, Eye, Folder, Image as ImageIcon, Link, Pencil, Search, Sparkle, Tag } from '../icons/Icon';
import { extractTitle, stripFirstH1 } from '../../lib/markdown';
import { saveStatusLabel, useAutosave } from '../../lib/useAutosave';
import { CalendarView, TodayEntry } from '../../lib/calendar';
import styles from './Reader.module.css';

interface Props {
  path: string;
  content: string;
  updated: string | null;
  // Initial editor mode — "edit" for freshly-created files, "preview" otherwise.
  initialMode?: 'preview' | 'edit';
  calendar: CalendarView;
  today: TodayEntry[];
  onSelectFile: (path: string) => void;
  onSelectDay: (day: number) => void;
  onNewEntry: () => void;
  onOpenVault: () => void;
  onLock: () => void;
  onSave: (path: string, content: string) => Promise<void>;
}

// Ported from dir-1-hearth.jsx:156-252 (HearthMain), extended for Phase 2 with
// an edit/preview toggle, debounced autosave, and an empty-state prompts
// overlay borrowed from HearthNew (dir-1-hearth.jsx:284-382).
export function Reader({
  path,
  content: initialContent,
  updated,
  initialMode = 'preview',
  calendar,
  today,
  onSelectFile,
  onSelectDay,
  onNewEntry,
  onOpenVault,
  onLock,
  onSave,
}: Props) {
  const [content, setContent] = useState(initialContent);
  const [mode, setMode] = useState<'preview' | 'edit'>(initialMode);
  const textareaRef = useRef<HTMLTextAreaElement | null>(null);

  // Reset local state when the user switches files.
  useEffect(() => {
    setContent(initialContent);
    setMode(initialMode);
  }, [path, initialContent, initialMode]);

  const saveFn = useMemo(
    () => async (value: string) => {
      await onSave(path, value);
    },
    [onSave, path],
  );
  const { status, markDirty, flush } = useAutosave({ onSave: saveFn });

  // Cmd/Ctrl+S to flush; nothing else.
  useEffect(() => {
    function onKey(e: KeyboardEvent) {
      if ((e.metaKey || e.ctrlKey) && e.key.toLowerCase() === 's') {
        e.preventDefault();
        void flush();
      }
    }
    window.addEventListener('keydown', onKey);
    return () => window.removeEventListener('keydown', onKey);
  }, [flush]);

  // Focus the textarea when first entering edit mode on a new file.
  useEffect(() => {
    if (mode === 'edit') {
      textareaRef.current?.focus();
    }
  }, [mode]);

  function onContentChange(e: ChangeEvent<HTMLTextAreaElement>) {
    const next = e.target.value;
    setContent(next);
    markDirty(next);
  }

  const segments = path.split('/');
  const fileName = segments[segments.length - 1] ?? path;
  const folderSegments = segments.slice(0, -1);

  const titleFromContent = extractTitle(content);
  const headlineTitle = titleFromContent ?? fileNameToTitle(fileName);
  const titleParts = splitTitle(headlineTitle);
  const bodySource = stripFirstH1(content);
  const wordCount = countWords(content);
  const readMin = Math.max(1, Math.round(wordCount / 220));
  const isEmpty = content.trim().length === 0;
  const saveLabel = saveStatusLabel(status);

  return (
    <HearthShell mode={mode === 'edit' ? 'editing' : 'reading'} onLock={onLock}>
      <div className={styles.layout}>
        <HearthRail
          calendar={calendar}
          today={today}
          onNewEntry={onNewEntry}
          onSelectFile={(p) => {
            void flush();
            onSelectFile(p);
          }}
          onSelectDay={onSelectDay}
          onOpenVault={() => {
            void flush();
            onOpenVault();
          }}
          activeSurface="reader"
        />
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
              {saveLabel && (
                <span
                  className={
                    status.kind === 'error'
                      ? `${styles.saveStatus} ${styles.saveStatusError}`
                      : status.kind === 'saved'
                      ? `${styles.saveStatus} ${styles.saveStatusOk}`
                      : styles.saveStatus
                  }
                >
                  <span className={styles.saveDot} aria-hidden /> {saveLabel}
                </span>
              )}
              <span className={styles.meta}>
                <Clock size={12} /> {readMin} min read
              </span>
              <span className={styles.meta}>{wordCount} words</span>
              <button
                type="button"
                className={`${styles.toolBtn} ${mode === 'preview' ? styles.toolBtnActive : ''}`}
                onClick={() => {
                  void flush();
                  setMode('preview');
                }}
              >
                <Eye size={12} /> Preview
              </button>
              <button
                type="button"
                className={`${styles.toolBtn} ${mode === 'edit' ? styles.toolBtnActive : ''}`}
                onClick={() => setMode('edit')}
              >
                <Pencil size={12} /> Edit
              </button>
            </div>
          </div>

          {mode === 'preview' && (
            <div className={styles.searchPill} aria-hidden>
              <Search size={12} />
              <span>Search the whole diary…</span>
              <kbd>⌘K</kbd>
            </div>
          )}

          <div className={styles.column}>
            <article className={styles.article}>
              <div className={styles.dateline}>
                <span>{formatDateline(updated)}</span>
                <span className={styles.datelineRule} />
                <span className={styles.tags}>
                  <Tag size={11} /> journal · {mode === 'edit' ? 'drafting' : 'morning-pages'}
                </span>
              </div>

              {mode === 'edit' && isEmpty ? (
                <div className={styles.placeholderTitle}>
                  A title for today…
                  <span className={styles.cursor} />
                </div>
              ) : (
                <h1 className={styles.title}>
                  {titleParts[0]}
                  {titleParts.length > 1 && (
                    <>
                      ,<br />
                      <em>{titleParts.slice(1).join(', ')}</em>
                    </>
                  )}
                </h1>
              )}

              {mode === 'edit' && isEmpty && (
                <div className={styles.prompts}>
                  <div className={styles.promptsLabel}>Three prompts, in case you're stuck</div>
                  {[
                    'What’s on the windowsill of your mind today?',
                    'Something small you noticed and want to keep.',
                    'A sentence you read this week that stayed.',
                  ].map((p, i) => (
                    <div key={i} className={styles.prompt}>
                      <span className={styles.promptIndex}>{i + 1}.</span>
                      {p}
                    </div>
                  ))}
                </div>
              )}

              {mode === 'preview' ? (
                <Markdown source={bodySource} />
              ) : (
                <textarea
                  ref={textareaRef}
                  className={styles.editor}
                  value={content}
                  onChange={onContentChange}
                  spellCheck
                  placeholder="Begin where you are."
                />
              )}

              {mode === 'preview' && !isEmpty && (
                <div className={styles.linkedFrom}>
                  <Link size={12} />
                  <span className={styles.linkedLabel}>Linked from</span>
                  <a className={styles.linkedLink}>On memory palaces</a>
                  <a className={styles.linkedLink}>Notes from M.</a>
                </div>
              )}
            </article>
          </div>

          {mode === 'edit' && (
            <div className={styles.inkBar} aria-hidden>
              <span className={styles.inkH1}>H1</span>
              <span className={styles.inkH2}>H2</span>
              <span className={styles.inkSep} />
              <strong className={styles.inkStrong}>B</strong>
              <em className={styles.inkEm}>I</em>
              <span className={styles.inkU}>U</span>
              <span className={styles.inkSep} />
              <span className={styles.inkMute}>“ ”</span>
              <span className={`${styles.inkMute} ${styles.inkMono}`}>{'</>'}</span>
              <ImageIcon size={13} />
              <Link size={13} />
              <span className={styles.inkSep} />
              <span className={styles.inkSparkle}>
                <Sparkle size={12} /> Improve
              </span>
            </div>
          )}
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
