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
  Pin,
  Search,
  Sparkle,
  Split,
  Tag,
} from '../icons/Icon';
import { extractTitle, stripFirstH1 } from '../../lib/markdown';
import { saveStatusLabel, useAutosave } from '../../lib/useAutosave';
import { CalendarView, TodayEntry } from '../../lib/calendar';
import styles from './Reader.module.css';

type ReaderMode = 'preview' | 'edit' | 'split';

interface Props {
  path: string;
  content: string;
  updated: string | null;
  initialMode?: ReaderMode;
  calendar: CalendarView;
  entries: TodayEntry[];
  entriesLabel: string;
  selectedDay: number | null;
  onClearSelectedDay: () => void;
  onPickDate: (value: string) => void;
  titleToPath: Map<string, string>;
  onSelectFile: (path: string) => void;
  onSelectDay: (day: number) => void;
  onNewEntry: () => void;
  onOpenVault: () => void;
  onOpenSearch: () => void;
  onLock: () => void;
  onSave: (path: string, content: string) => Promise<void>;
  onCheckpoint?: (path: string, content: string, message?: string) => Promise<void>;
  onRollback?: (path: string, commit: string) => Promise<void>;
  onWikilinkMiss?: (title: string) => void;
  onOpenPasskey?: () => void;
  hasPasskey?: boolean;
  theme?: 'light' | 'dark';
  onToggleTheme?: () => void;
}

const WIKILINK_MAX_SUGGESTIONS = 6;

interface AutocompleteState {
  open: boolean;
  /** Character index of the first char after `[[`. */
  start: number;
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
  onPickDate,
  titleToPath,
  onSelectFile,
  onSelectDay,
  onNewEntry,
  onOpenVault,
  onOpenSearch,
  onLock,
  onSave,
  onCheckpoint,
  onRollback,
  onWikilinkMiss,
  onOpenPasskey,
  hasPasskey,
  theme,
  onToggleTheme,
}: Props) {
  const [content, setContent] = useState(initialContent);
  const [mode, setMode] = useState<ReaderMode>(initialMode);
  const [historyOpen, setHistoryOpen] = useState(false);
  const [ac, setAc] = useState<AutocompleteState>(EMPTY_AC);
  const textareaRef = useRef<HTMLTextAreaElement | null>(null);

  const [checkpointOpen, setCheckpointOpen] = useState(false);
  const [checkpointLabel, setCheckpointLabel] = useState('');
  const [checkpointing, setCheckpointing] = useState(false);
  const [checkpointError, setCheckpointError] = useState<string | null>(null);
  const [dirtySinceCheckpoint, setDirtySinceCheckpoint] = useState(false);
  const [historyReloadToken, setHistoryReloadToken] = useState(0);
  const checkpointWrapRef = useRef<HTMLDivElement | null>(null);

  useEffect(() => {
    setContent(initialContent);
    setMode(initialMode);
    setAc(EMPTY_AC);
    setCheckpointOpen(false);
    setCheckpointLabel('');
    setCheckpointError(null);
    setDirtySinceCheckpoint(false);
  }, [path, initialContent, initialMode]);

  const saveFn = useMemo(
    () => async (value: string) => {
      await onSave(path, value);
    },
    [onSave, path],
  );
  const { status, markDirty, flush } = useAutosave({ onSave: saveFn });

  /** Wrap `markDirty` so every edit also flags changes not yet checkpointed. */
  const touch = useCallback(
    (next: string) => {
      markDirty(next);
      setDirtySinceCheckpoint(true);
    },
    [markDirty],
  );

  const doCheckpoint = useCallback(async () => {
    if (!onCheckpoint || checkpointing) { return; }
    setCheckpointing(true);
    setCheckpointError(null);
    try {
      await onCheckpoint(path, content, checkpointLabel.trim() || undefined);
      setDirtySinceCheckpoint(false);
      setCheckpointOpen(false);
      setCheckpointLabel('');
      setHistoryReloadToken((token) => token + 1);
    } catch (err) {
      setCheckpointError(err instanceof Error ? err.message : 'checkpoint failed');
    } finally {
      setCheckpointing(false);
    }
  }, [checkpointLabel, checkpointing, content, onCheckpoint, path]);

  useEffect(() => {
    if (!checkpointOpen) { return; }
    function onPointerDown(event: PointerEvent) {
      if (!checkpointWrapRef.current?.contains(event.target as Node)) {
        setCheckpointOpen(false);
      }
    }
    window.addEventListener('pointerdown', onPointerDown);
    return () => window.removeEventListener('pointerdown', onPointerDown);
  }, [checkpointOpen]);

  useEffect(() => {
    function onKey(event: KeyboardEvent) {
      if ((event.metaKey || event.ctrlKey) && event.key.toLowerCase() === 's') {
        event.preventDefault();
        void flush();
      }
    }
    window.addEventListener('keydown', onKey);
    return () => window.removeEventListener('keydown', onKey);
  }, [flush]);

  /** Apply a transform to the textarea selection: update content via state, then
   *  restore the caret on the next frame so the controlled textarea doesn't fight
   *  us. The shared primitive behind every editor formatting helper. */
  const applyToSelection = useCallback(
    (transform: (value: string, start: number, end: number) => { text: string; selStart: number; selEnd: number }) => {
      const textarea = textareaRef.current;
      if (!textarea) { return; }
      const { selectionStart, selectionEnd, value } = textarea;
      const { text, selStart, selEnd } = transform(value, selectionStart, selectionEnd);
      setContent(text);
      touch(text);
      window.requestAnimationFrame(() => {
        textarea.focus();
        textarea.setSelectionRange(selStart, selEnd);
      });
    },
    [touch],
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
        let lineStart = start;
        while (lineStart > 0 && value[lineStart - 1] !== '\n') { lineStart -= 1; }
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
    if (mode === 'edit' || mode === 'split') { textareaRef.current?.focus(); }
  }, [mode]);

  /** Pre-lowered titles so the autocomplete filter doesn't lowercase the whole
   *  corpus on every keystroke; rebuilt only when `titleToPath` changes. */
  const titleIndex = useMemo(() => {
    const list: { title: string; lower: string }[] = [];
    for (const title of titleToPath.keys()) {
      list.push({ title, lower: title.toLowerCase() });
    }
    return list;
  }, [titleToPath]);

  /** Find the most recent `[[` to the left of the caret with no closing `]]` or
   *  newline between, and open the autocomplete with matching note titles. */
  function recomputeAutocomplete(value: string, caret: number) {
    let i = caret - 1;
    while (i >= 0) {
      const ch = value[i];
      if (ch === '\n') {
        setAc(EMPTY_AC);
        return;
      }
      if (i >= 1 && value[i - 1] === '[' && value[i] === '[') {
        const start = i + 1;
        const between = value.slice(start, caret);
        if (between.includes(']') || between.includes('[')) {
          setAc(EMPTY_AC);
          return;
        }
        const query = between.toLowerCase();
        const hits: string[] = [];
        for (const entry of titleIndex) {
          if (entry.lower.includes(query)) {
            hits.push(entry.title);
            if (hits.length >= WIKILINK_MAX_SUGGESTIONS) { break; }
          }
        }
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

  function onContentChange(event: ChangeEvent<HTMLTextAreaElement>) {
    const next = event.target.value;
    setContent(next);
    touch(next);
    recomputeAutocomplete(next, event.target.selectionStart);
  }

  function insertSuggestion(title: string) {
    const textarea = textareaRef.current;
    if (!textarea) { return; }
    const start = ac.start;
    if (start < 0) { return; }
    const before = content.slice(0, start);
    const after = content.slice(start + ac.query.length);
    const insertText = `${title}]]`;
    const next = `${before}${insertText}${after}`;
    setContent(next);
    touch(next);
    setAc(EMPTY_AC);
    const caret = before.length + insertText.length;
    window.requestAnimationFrame(() => {
      textarea.focus();
      textarea.setSelectionRange(caret, caret);
    });
  }

  function onTextareaKeyDown(event: ReactKeyboardEvent<HTMLTextAreaElement>) {
    if (event.metaKey || event.ctrlKey) {
      const key = event.key.toLowerCase();
      if (key === 'b') {
        event.preventDefault();
        wrapSelection('**');
        return;
      }
      if (key === 'i') {
        event.preventDefault();
        wrapSelection('*');
        return;
      }
    }
    if (!ac.open) { return; }
    if (event.key === 'ArrowDown') {
      event.preventDefault();
      setAc((state) => ({ ...state, activeIdx: Math.min(state.hits.length - 1, state.activeIdx + 1) }));
    } else if (event.key === 'ArrowUp') {
      event.preventDefault();
      setAc((state) => ({ ...state, activeIdx: Math.max(0, state.activeIdx - 1) }));
    } else if (event.key === 'Enter' || event.key === 'Tab') {
      event.preventDefault();
      const title = ac.hits[ac.activeIdx];
      if (title) { insertSuggestion(title); }
    } else if (event.key === 'Escape') {
      event.preventDefault();
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

  /** Flush-then-select for the rail. Kept referentially stable (alongside
   *  `railOpenVault`) so the memoized `HearthRail` doesn't re-render on every
   *  keystroke. */
  const railSelectFile = useCallback(
    (selectedPath: string) => {
      void flush();
      onSelectFile(selectedPath);
    },
    [flush, onSelectFile],
  );
  const railOpenVault = useCallback(() => {
    void flush();
    onOpenVault();
  }, [flush, onOpenVault]);

  const { fileName, folderSegments } = useMemo(() => {
    const segments = path.split('/');
    return {
      fileName: segments[segments.length - 1] ?? path,
      folderSegments: segments.slice(0, -1),
    };
  }, [path]);

  const titleFromContent = useMemo(() => extractTitle(content), [content]);
  const wordCount = useMemo(() => countWords(content), [content]);
  const bodySource = useMemo(
    () => (mode === 'preview' || mode === 'split' ? stripFirstH1(content) : ''),
    [content, mode],
  );
  const linkedTitles = useMemo(
    () =>
      mode === 'preview' ? backlinkableTitles(titleFromContent, content, titleToPath) : [],
    [content, mode, titleFromContent, titleToPath],
  );

  const headlineTitle = titleFromContent ?? fileNameToTitle(fileName);
  const titleParts = splitTitle(headlineTitle);
  const readMin = Math.max(1, Math.round(wordCount / 220));
  const isEmpty = content.length === 0 || content.trim().length === 0;
  const saveLabel = saveStatusLabel(status);

  const editorPane = (
    <div className={styles.editorWrap}>
      <textarea
        ref={textareaRef}
        className={styles.editor}
        value={content}
        onChange={onContentChange}
        onKeyDown={onTextareaKeyDown}
        onClick={(event) => recomputeAutocomplete(content, event.currentTarget.selectionStart)}
        onSelect={(event) => recomputeAutocomplete(content, event.currentTarget.selectionStart)}
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
              onMouseEnter={() => setAc((state) => ({ ...state, activeIdx: i }))}
              onMouseDown={(event) => {
                event.preventDefault();
                insertSuggestion(title);
              }}
            >
              {title}
            </button>
          ))}
        </div>
      )}
    </div>
  );

  const datelineBlock = (
    <div className={styles.dateline}>
      <span>{formatDateline(updated)}</span>
      <span className={styles.datelineRule} />
      <span className={styles.tags}>
        <Tag size={11} /> journal · {mode === 'preview' ? 'morning-pages' : 'drafting'}
      </span>
    </div>
  );

  const titleHeadline = (
    <h1 className={styles.title}>
      {titleParts[0]}
      {titleParts.length > 1 && (
        <>
          ,<br />
          <em>{titleParts.slice(1).join(', ')}</em>
        </>
      )}
    </h1>
  );

  return (
    <HearthShell
      mode={mode === 'preview' ? 'reading' : 'editing'}
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
          onPickDate={onPickDate}
          onNewEntry={onNewEntry}
          onSelectFile={railSelectFile}
          onSelectDay={onSelectDay}
          onOpenVault={railOpenVault}
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
              {onCheckpoint && (
                <div className={styles.checkpointWrap} ref={checkpointWrapRef}>
                  <button
                    type="button"
                    className={`${styles.toolBtn} ${checkpointOpen ? styles.toolBtnActive : ''}`}
                    onClick={() => {
                      setCheckpointError(null);
                      setCheckpointOpen((open) => !open);
                    }}
                    title="Save a checkpoint you can return to"
                  >
                    <Pin size={12} /> Checkpoint
                    {dirtySinceCheckpoint && (
                      <span className={styles.checkpointDot} aria-label="unsaved changes" />
                    )}
                  </button>
                  {checkpointOpen && (
                    <div className={styles.checkpointPopover} role="dialog" aria-label="Save checkpoint">
                      <div className={styles.checkpointHint}>Name this checkpoint — text or emoji 🔖</div>
                      <input
                        className={styles.checkpointInput}
                        value={checkpointLabel}
                        onChange={(event) => setCheckpointLabel(event.target.value)}
                        onKeyDown={(event) => {
                          if (event.key === 'Enter') {
                            event.preventDefault();
                            void doCheckpoint();
                          } else if (event.key === 'Escape') {
                            event.preventDefault();
                            setCheckpointOpen(false);
                          }
                        }}
                        placeholder="Optional label"
                        maxLength={120}
                        autoFocus
                      />
                      {checkpointError && (
                        <div className={styles.checkpointError}>{checkpointError}</div>
                      )}
                      <div className={styles.checkpointActions}>
                        <button
                          type="button"
                          className={styles.checkpointCancel}
                          onClick={() => setCheckpointOpen(false)}
                        >
                          Cancel
                        </button>
                        <button
                          type="button"
                          className={styles.checkpointSave}
                          onClick={() => void doCheckpoint()}
                          disabled={checkpointing}
                        >
                          {checkpointing ? 'Saving…' : 'Save checkpoint'}
                        </button>
                      </div>
                    </div>
                  )}
                </div>
              )}
              <button
                type="button"
                className={`${styles.toolBtn} ${historyOpen ? styles.toolBtnActive : ''}`}
                onClick={() => setHistoryOpen((open) => !open)}
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
                className={`${styles.toolBtn} ${mode === 'split' ? styles.toolBtnActive : ''}`}
                onClick={() => setMode('split')}
                title="Editor + preview side by side"
              >
                <Split size={12} /> Split
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
            {mode === 'split' ? (
              <div className={styles.splitRow}>
                <div className={`${styles.column} ${styles.splitPaneLeft}`}>
                  <article className={`${styles.article} ${styles.articleSplit}`}>
                    {datelineBlock}
                    {titleHeadline}
                    {editorPane}
                  </article>
                </div>
                <div className={`${styles.column} ${styles.splitPaneRight}`}>
                  <article className={`${styles.article} ${styles.articleSplit}`}>
                    {datelineBlock}
                    {titleHeadline}
                    <Markdown source={bodySource} onWikilinkClick={handleWikilinkClick} />
                  </article>
                </div>
              </div>
            ) : (
              <div className={styles.column}>
                <article className={styles.article}>
                  {datelineBlock}

                  {mode === 'edit' && isEmpty ? (
                    <div className={styles.placeholderTitle}>
                      A title for today…
                      <span className={styles.cursor} />
                    </div>
                  ) : (
                    titleHeadline
                  )}

                  {mode === 'edit' && isEmpty && (
                    <div className={styles.prompts}>
                      <div className={styles.promptsLabel}>Three prompts, in case you’re stuck</div>
                      {[
                        'What’s on the windowsill of your mind today?',
                        'Something small you noticed and want to keep.',
                        'A sentence you read this week that stayed.',
                      ].map((prompt, i) => (
                        <div key={i} className={styles.prompt}>
                          <span className={styles.promptIndex}>{i + 1}.</span>
                          {prompt}
                        </div>
                      ))}
                    </div>
                  )}

                  {mode === 'edit' && editorPane}

                  {mode === 'preview' && !isEmpty && (
                    <Markdown source={bodySource} onWikilinkClick={handleWikilinkClick} />
                  )}

                  {mode === 'preview' && !isEmpty && linkedTitles.length > 0 && (
                    <div className={styles.linkedFrom}>
                      <Link size={12} />
                      <span className={styles.linkedLabel}>Linked notes</span>
                      {linkedTitles.map((title) => (
                        <button
                          key={title}
                          type="button"
                          className={styles.linkedLink}
                          onClick={() => handleWikilinkClick(title)}
                        >
                          {title}
                        </button>
                      ))}
                    </div>
                  )}
                </article>
              </div>
            )}

            {historyOpen && (
              <HistoryPanel
                open={historyOpen}
                path={path}
                reloadToken={historyReloadToken}
                onClose={() => setHistoryOpen(false)}
                onRollback={onRollback}
              />
            )}
          </div>

          {(mode === 'edit' || mode === 'split') && (
            <div className={styles.inkBar}>
              <button
                type="button"
                className={styles.inkBtn}
                onMouseDown={(event) => {
                  event.preventDefault();
                  prefixLine('# ');
                }}
                title="Heading 1"
              >
                <span className={styles.inkH1}>H1</span>
              </button>
              <button
                type="button"
                className={styles.inkBtn}
                onMouseDown={(event) => {
                  event.preventDefault();
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
                onMouseDown={(event) => {
                  event.preventDefault();
                  wrapSelection('**');
                }}
                title="Bold (⌘B)"
              >
                <strong className={styles.inkStrong}>B</strong>
              </button>
              <button
                type="button"
                className={styles.inkBtn}
                onMouseDown={(event) => {
                  event.preventDefault();
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
                onMouseDown={(event) => {
                  event.preventDefault();
                  prefixLine('> ');
                }}
                title="Quote"
              >
                <span className={styles.inkMute}>“ ”</span>
              </button>
              <button
                type="button"
                className={styles.inkBtn}
                onMouseDown={(event) => {
                  event.preventDefault();
                  wrapSelection('`');
                }}
                title="Inline code"
              >
                <span className={`${styles.inkMute} ${styles.inkMono}`}>{'</>'}</span>
              </button>
              <button
                type="button"
                className={styles.inkBtn}
                onMouseDown={(event) => {
                  event.preventDefault();
                  insertAtCursor('![](url)', 2);
                }}
                title="Image"
              >
                <ImageIcon size={13} />
              </button>
              <button
                type="button"
                className={styles.inkBtn}
                onMouseDown={(event) => {
                  event.preventDefault();
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
  const parts = title.split(',').map((part) => part.trim());
  return parts.length > 1 ? parts : [title];
}

function fileNameToTitle(name: string): string {
  return name.replace(/\.(md|markdown)$/i, '');
}

function countWords(src: string): number {
  return src
    .split(/\s+/)
    .filter((word) => /\w/.test(word))
    .length;
}

function formatDateline(updated: string | null): string {
  const date = updated ? new Date(updated) : new Date();
  const weekday = date.toLocaleDateString(undefined, { weekday: 'long' });
  const day = date.toLocaleDateString(undefined, { day: 'numeric', month: 'long', year: 'numeric' });
  const time = date.toLocaleTimeString(undefined, { hour: '2-digit', minute: '2-digit', hour12: false });
  return `${weekday} · ${day} · ${time}`;
}

/** Wikilinked titles in the content that resolve to real notes, excluding
 * self-references. */
function backlinkableTitles(
  currentTitle: string | null,
  content: string,
  titleToPath: Map<string, string>,
): string[] {
  const found = new Set<string>();
  const pattern = /\[\[([^\]]+)\]\]/g;
  let match: RegExpExecArray | null;
  while ((match = pattern.exec(content)) !== null) {
    const title = match[1].trim();
    if (currentTitle && title === currentTitle) { continue; }
    if (titleToPath.has(title)) { found.add(title); }
  }
  return Array.from(found);
}
