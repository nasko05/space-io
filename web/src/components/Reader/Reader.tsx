import {
  ChangeEvent,
  KeyboardEvent as ReactKeyboardEvent,
  useCallback,
  useEffect,
  useMemo,
  useRef,
  useState,
} from 'react';
import { HearthShell } from '../Shell/HearthShell';
import { HearthRail } from '../Rail/HearthRail';
import { Markdown } from '../Markdown/Markdown';
import { HistoryPanel } from '../History/HistoryPanel';
import {
  Branch,
  Chevron,
  Clock,
  Eye,
  Folder,
  Image as ImageIcon,
  Link,
  Pencil,
  Search,
  Sparkle,
  Tag,
} from '../icons/Icon';
import { extractTitle, stripFirstH1 } from '../../lib/markdown';
import { saveStatusLabel, useAutosave } from '../../lib/useAutosave';
import { CalendarView, TodayEntry } from '../../lib/calendar';
import styles from './Reader.module.css';

interface Props {
  path: string;
  content: string;
  updated: string | null;
  initialMode?: 'preview' | 'edit';
  calendar: CalendarView;
  entries: TodayEntry[];
  entriesLabel: string;
  selectedDay: number | null;
  onClearSelectedDay: () => void;
  titleToPath: Map<string, string>;
  onSelectFile: (path: string) => void;
  onSelectDay: (day: number) => void;
  onNewEntry: () => void;
  onOpenVault: () => void;
  onOpenSearch: () => void;
  onLock: () => void;
  onSave: (path: string, content: string) => Promise<void>;
  onWikilinkMiss?: (title: string) => void;
  onOpenPasskey?: () => void;
  hasPasskey?: boolean;
  theme?: 'light' | 'dark';
  onToggleTheme?: () => void;
}

const WIKILINK_MAX_SUGGESTIONS = 6;

interface AutocompleteState {
  open: boolean;
  start: number; // character index of the first char after `[[`
  query: string;
  hits: string[];
  activeIdx: number;
}

const EMPTY_AC: AutocompleteState = {
  open: false,
  start: -1,
  query: '',
  hits: [],
  activeIdx: 0,
};

export function Reader({
  path,
  content: initialContent,
  updated,
  initialMode = 'preview',
  calendar,
  entries,
  entriesLabel,
  selectedDay,
  onClearSelectedDay,
  titleToPath,
  onSelectFile,
  onSelectDay,
  onNewEntry,
  onOpenVault,
  onOpenSearch,
  onLock,
  onSave,
  onWikilinkMiss,
  onOpenPasskey,
  hasPasskey,
  theme,
  onToggleTheme,
}: Props) {
  const [content, setContent] = useState(initialContent);
  const [mode, setMode] = useState<'preview' | 'edit'>(initialMode);
  const [historyOpen, setHistoryOpen] = useState(false);
  const [ac, setAc] = useState<AutocompleteState>(EMPTY_AC);
  const textareaRef = useRef<HTMLTextAreaElement | null>(null);

  useEffect(() => {
    setContent(initialContent);
    setMode(initialMode);
    setAc(EMPTY_AC);
  }, [path, initialContent, initialMode]);

  const saveFn = useMemo(
    () => async (value: string) => {
      await onSave(path, value);
    },
    [onSave, path],
  );
  const { status, markDirty, flush } = useAutosave({ onSave: saveFn });

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

  // Editor-side formatting helpers. They mutate the textarea content
  // through state + caret restoration on the next frame so React's
  // controlled input doesn't fight us.
  const applyToSelection = useCallback(
    (transform: (value: string, start: number, end: number) => { text: string; selStart: number; selEnd: number }) => {
      const ta = textareaRef.current;
      if (!ta) return;
      const { selectionStart, selectionEnd, value } = ta;
      const { text, selStart, selEnd } = transform(value, selectionStart, selectionEnd);
      setContent(text);
      markDirty(text);
      window.requestAnimationFrame(() => {
        ta.focus();
        ta.setSelectionRange(selStart, selEnd);
      });
    },
    [markDirty],
  );

  const wrapSelection = useCallback(
    (marker: string) =>
      applyToSelection((value, start, end) => {
        const before = value.slice(0, start);
        const sel = value.slice(start, end);
        const after = value.slice(end);
        const text = `${before}${marker}${sel}${marker}${after}`;
        return {
          text,
          selStart: start + marker.length,
          selEnd: end + marker.length,
        };
      }),
    [applyToSelection],
  );

  const prefixLine = useCallback(
    (prefix: string) =>
      applyToSelection((value, start, end) => {
        // Find the start of the current line.
        let lineStart = start;
        while (lineStart > 0 && value[lineStart - 1] !== '\n') lineStart -= 1;
        const before = value.slice(0, lineStart);
        const rest = value.slice(lineStart);
        const text = `${before}${prefix}${rest}`;
        return {
          text,
          selStart: start + prefix.length,
          selEnd: end + prefix.length,
        };
      }),
    [applyToSelection],
  );

  const insertAtCursor = useCallback(
    (snippet: string, caretOffset?: number) =>
      applyToSelection((value, start, end) => {
        const before = value.slice(0, start);
        const after = value.slice(end);
        const text = `${before}${snippet}${after}`;
        const caret = start + (caretOffset ?? snippet.length);
        return { text, selStart: caret, selEnd: caret };
      }),
    [applyToSelection],
  );

  useEffect(() => {
    if (mode === 'edit') textareaRef.current?.focus();
  }, [mode]);

  const allTitles = useMemo(() => Array.from(titleToPath.keys()), [titleToPath]);

  function recomputeAutocomplete(value: string, caret: number) {
    // Find the most recent `[[` to the left of the caret with no closing `]]`
    // or whitespace between.
    let i = caret - 1;
    while (i >= 0) {
      const ch = value[i];
      if (ch === '\n') {
        setAc(EMPTY_AC);
        return;
      }
      if (i >= 1 && value[i - 1] === '[' && value[i] === '[') {
        // i points at the second `[`. Query starts at i + 1.
        const start = i + 1;
        const between = value.slice(start, caret);
        if (between.includes(']') || between.includes('[')) {
          setAc(EMPTY_AC);
          return;
        }
        const q = between.toLowerCase();
        const hits = allTitles
          .filter((t) => t.toLowerCase().includes(q))
          .slice(0, WIKILINK_MAX_SUGGESTIONS);
        if (hits.length === 0) {
          setAc(EMPTY_AC);
          return;
        }
        setAc({ open: true, start, query: between, hits, activeIdx: 0 });
        return;
      }
      i -= 1;
    }
    setAc(EMPTY_AC);
  }

  function onContentChange(e: ChangeEvent<HTMLTextAreaElement>) {
    const next = e.target.value;
    setContent(next);
    markDirty(next);
    recomputeAutocomplete(next, e.target.selectionStart);
  }

  function insertSuggestion(title: string) {
    const ta = textareaRef.current;
    if (!ta) return;
    const start = ac.start;
    if (start < 0) return;
    const before = content.slice(0, start);
    // Replace the in-progress query and add a closing `]]`.
    const after = content.slice(start + ac.query.length);
    const insertText = `${title}]]`;
    const next = `${before}${insertText}${after}`;
    setContent(next);
    markDirty(next);
    setAc(EMPTY_AC);
    // Restore caret to just after the inserted `]]`.
    const caret = before.length + insertText.length;
    window.requestAnimationFrame(() => {
      ta.focus();
      ta.setSelectionRange(caret, caret);
    });
  }

  function onTextareaKeyDown(e: ReactKeyboardEvent<HTMLTextAreaElement>) {
    // Formatting shortcuts work whether or not autocomplete is open.
    if (e.metaKey || e.ctrlKey) {
      const key = e.key.toLowerCase();
      if (key === 'b') {
        e.preventDefault();
        wrapSelection('**');
        return;
      }
      if (key === 'i') {
        e.preventDefault();
        wrapSelection('*');
        return;
      }
    }
    if (!ac.open) return;
    if (e.key === 'ArrowDown') {
      e.preventDefault();
      setAc((s) => ({ ...s, activeIdx: Math.min(s.hits.length - 1, s.activeIdx + 1) }));
    } else if (e.key === 'ArrowUp') {
      e.preventDefault();
      setAc((s) => ({ ...s, activeIdx: Math.max(0, s.activeIdx - 1) }));
    } else if (e.key === 'Enter' || e.key === 'Tab') {
      e.preventDefault();
      const title = ac.hits[ac.activeIdx];
      if (title) insertSuggestion(title);
    } else if (e.key === 'Escape') {
      e.preventDefault();
      setAc(EMPTY_AC);
    }
  }

  const handleWikilinkClick = useCallback(
    (title: string) => {
      const target = titleToPath.get(title);
      if (target) {
        void flush();
        onSelectFile(target);
      } else {
        onWikilinkMiss?.(title);
      }
    },
    [flush, onSelectFile, onWikilinkMiss, titleToPath],
  );

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
    <HearthShell
      mode={mode === 'edit' ? 'editing' : 'reading'}
      onLock={onLock}
      theme={theme}
      onToggleTheme={onToggleTheme}
    >
      <div className={styles.layout}>
        <HearthRail
          calendar={calendar}
          entries={entries}
          entriesLabel={entriesLabel}
          selectedDay={selectedDay}
          onClearSelectedDay={onClearSelectedDay}
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
          onOpenPasskey={onOpenPasskey}
          hasPasskey={hasPasskey}
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
                className={styles.toolBtn}
                onClick={onOpenSearch}
                aria-label="Search"
                title="Search (⌘K)"
              >
                <Search size={12} /> <kbd className={styles.kbd}>⌘K</kbd>
              </button>
              <button
                type="button"
                className={`${styles.toolBtn} ${historyOpen ? styles.toolBtnActive : ''}`}
                onClick={() => setHistoryOpen((v) => !v)}
                title="Version history"
              >
                <Branch size={12} /> History
              </button>
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

          <div className={styles.contentRow}>
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
                  <Markdown source={bodySource} onWikilinkClick={handleWikilinkClick} />
                ) : (
                  <div className={styles.editorWrap}>
                    <textarea
                      ref={textareaRef}
                      className={styles.editor}
                      value={content}
                      onChange={onContentChange}
                      onKeyDown={onTextareaKeyDown}
                      onClick={(e) => recomputeAutocomplete(content, e.currentTarget.selectionStart)}
                      onSelect={(e) => recomputeAutocomplete(content, e.currentTarget.selectionStart)}
                      onBlur={() => window.setTimeout(() => setAc(EMPTY_AC), 120)}
                      spellCheck
                      placeholder="Begin where you are."
                    />
                    {ac.open && (
                      <div className={styles.autocomplete} role="listbox">
                        <div className={styles.autocompleteLabel}>
                          Link to a note · ↑↓ to choose, ↵ to insert, esc to dismiss
                        </div>
                        {ac.hits.map((title, i) => (
                          <button
                            key={title}
                            type="button"
                            role="option"
                            aria-selected={i === ac.activeIdx}
                            className={`${styles.autoItem} ${i === ac.activeIdx ? styles.autoItemActive : ''}`}
                            onMouseEnter={() => setAc((s) => ({ ...s, activeIdx: i }))}
                            onMouseDown={(e) => {
                              e.preventDefault();
                              insertSuggestion(title);
                            }}
                          >
                            {title}
                          </button>
                        ))}
                      </div>
                    )}
                  </div>
                )}

                {mode === 'preview' && !isEmpty && backlinkableTitles(titleFromContent, content, titleToPath).length > 0 && (
                  <div className={styles.linkedFrom}>
                    <Link size={12} />
                    <span className={styles.linkedLabel}>Linked notes</span>
                    {backlinkableTitles(titleFromContent, content, titleToPath).map((t) => (
                      <button
                        key={t}
                        type="button"
                        className={styles.linkedLink}
                        onClick={() => handleWikilinkClick(t)}
                      >
                        {t}
                      </button>
                    ))}
                  </div>
                )}
              </article>
            </div>

            {historyOpen && (
              <HistoryPanel
                open={historyOpen}
                path={path}
                onClose={() => setHistoryOpen(false)}
              />
            )}
          </div>

          {mode === 'edit' && (
            <div className={styles.inkBar}>
              <button
                type="button"
                className={styles.inkBtn}
                onMouseDown={(e) => {
                  e.preventDefault();
                  prefixLine('# ');
                }}
                title="Heading 1"
              >
                <span className={styles.inkH1}>H1</span>
              </button>
              <button
                type="button"
                className={styles.inkBtn}
                onMouseDown={(e) => {
                  e.preventDefault();
                  prefixLine('## ');
                }}
                title="Heading 2"
              >
                <span className={styles.inkH2}>H2</span>
              </button>
              <span className={styles.inkSep} />
              <button
                type="button"
                className={styles.inkBtn}
                onMouseDown={(e) => {
                  e.preventDefault();
                  wrapSelection('**');
                }}
                title="Bold (⌘B)"
              >
                <strong className={styles.inkStrong}>B</strong>
              </button>
              <button
                type="button"
                className={styles.inkBtn}
                onMouseDown={(e) => {
                  e.preventDefault();
                  wrapSelection('*');
                }}
                title="Italic (⌘I)"
              >
                <em className={styles.inkEm}>I</em>
              </button>
              <span className={styles.inkSep} />
              <button
                type="button"
                className={styles.inkBtn}
                onMouseDown={(e) => {
                  e.preventDefault();
                  prefixLine('> ');
                }}
                title="Quote"
              >
                <span className={styles.inkMute}>“ ”</span>
              </button>
              <button
                type="button"
                className={styles.inkBtn}
                onMouseDown={(e) => {
                  e.preventDefault();
                  wrapSelection('`');
                }}
                title="Inline code"
              >
                <span className={`${styles.inkMute} ${styles.inkMono}`}>{'</>'}</span>
              </button>
              <button
                type="button"
                className={styles.inkBtn}
                onMouseDown={(e) => {
                  e.preventDefault();
                  insertAtCursor('![](url)', 2);
                }}
                title="Image"
              >
                <ImageIcon size={13} />
              </button>
              <button
                type="button"
                className={styles.inkBtn}
                onMouseDown={(e) => {
                  e.preventDefault();
                  insertAtCursor('[](url)', 1);
                }}
                title="Link"
              >
                <Link size={13} />
              </button>
              <span className={styles.inkSep} />
              <span
                className={`${styles.inkBtn} ${styles.inkSparkle}`}
                title="AI-improve (coming soon)"
              >
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

/** Extract the wikilinked titles in the current content that resolve to
 * actual notes, excluding self-references. */
function backlinkableTitles(
  currentTitle: string | null,
  content: string,
  titleToPath: Map<string, string>,
): string[] {
  const found = new Set<string>();
  const re = /\[\[([^\]]+)\]\]/g;
  let m: RegExpExecArray | null;
  while ((m = re.exec(content)) !== null) {
    const t = m[1].trim();
    if (currentTitle && t === currentTitle) continue;
    if (titleToPath.has(t)) found.add(t);
  }
  return Array.from(found);
}
