import { ChangeEvent, useEffect, useMemo, useRef, useState } from 'react';
import { api, SearchHit } from '../../api/client';
import { Search } from '../icons/Icon';
import styles from './SearchOverlay.module.css';

interface Props {
  open: boolean;
  onClose: () => void;
  onSelect: (path: string) => void;
}

export function SearchOverlay({ open, onClose, onSelect }: Props) {
  const [query, setQuery] = useState('');
  const [hits, setHits] = useState<SearchHit[]>([]);
  const [busy, setBusy] = useState(false);
  const [activeIndex, setActiveIndex] = useState(0);
  const inputRef = useRef<HTMLInputElement | null>(null);

  // Focus the input each time the overlay opens.
  useEffect(() => {
    if (open) {
      setQuery('');
      setHits([]);
      setActiveIndex(0);
      // Defer to next tick so the input exists.
      const t = window.setTimeout(() => inputRef.current?.focus(), 0);
      return () => window.clearTimeout(t);
    }
    return undefined;
  }, [open]);

  // Debounced query.
  useEffect(() => {
    if (!open) return;
    const trimmed = query.trim();
    if (!trimmed) {
      setHits([]);
      return;
    }
    let cancelled = false;
    const t = window.setTimeout(async () => {
      setBusy(true);
      try {
        const { hits } = await api.search(trimmed);
        if (!cancelled) {
          setHits(hits);
          setActiveIndex(0);
        }
      } catch (err) {
        if (!cancelled) {
          console.error('search failed', err);
          setHits([]);
        }
      } finally {
        if (!cancelled) setBusy(false);
      }
    }, 180);
    return () => {
      cancelled = true;
      window.clearTimeout(t);
    };
  }, [query, open]);

  function onKey(e: React.KeyboardEvent) {
    if (e.key === 'Escape') {
      e.preventDefault();
      onClose();
      return;
    }
    if (hits.length === 0) return;
    if (e.key === 'ArrowDown') {
      e.preventDefault();
      setActiveIndex((i) => Math.min(hits.length - 1, i + 1));
    } else if (e.key === 'ArrowUp') {
      e.preventDefault();
      setActiveIndex((i) => Math.max(0, i - 1));
    } else if (e.key === 'Enter') {
      e.preventDefault();
      const hit = hits[activeIndex];
      if (hit) {
        onSelect(hit.path);
      }
    }
  }

  // Build the highlight regex once per query rather than once per (hit ×
  // field). With 24 hits × 2 fields per hit, the old highlight() was
  // re-compiling the regex 48 times per render of the results pane.
  const highlightPattern = useMemo(() => {
    const tokens = query
      .trim()
      .split(/\s+/)
      .filter((t) => t.length > 0);
    if (tokens.length === 0) return null;
    return new RegExp(`(${tokens.map(escapeRegex).join('|')})`, 'gi');
  }, [query]);

  if (!open) return null;

  return (
    <div className={styles.scrim} onMouseDown={onClose}>
      <div className={styles.panel} onMouseDown={(e) => e.stopPropagation()}>
        <div className={styles.inputRow}>
          <Search size={16} />
          <input
            ref={inputRef}
            className={styles.input}
            value={query}
            onChange={(e: ChangeEvent<HTMLInputElement>) => setQuery(e.target.value)}
            onKeyDown={onKey}
            placeholder="Search the whole space…"
            spellCheck={false}
            autoComplete="off"
          />
          <kbd className={styles.kbd}>esc</kbd>
        </div>

        <div className={styles.results}>
          {query.trim() && !busy && hits.length === 0 && (
            <div className={styles.empty}>
              <em>Nothing matches.</em> Try a different word, or write a new note.
            </div>
          )}
          {!query.trim() && (
            <div className={styles.empty}>
              <em>Search across titles, body, and tags.</em>
              <br />
              <span className={styles.emptyHint}>Type to begin. ↵ to open, ↑↓ to move, esc to close.</span>
            </div>
          )}
          {hits.map((hit, i) => {
            const title = hit.title ?? hit.path.split('/').pop() ?? hit.path;
            return (
              <button
                type="button"
                key={hit.path}
                className={`${styles.hit} ${i === activeIndex ? styles.hitActive : ''}`}
                onMouseEnter={() => setActiveIndex(i)}
                onClick={() => onSelect(hit.path)}
              >
                <div className={styles.hitTitle}>{highlight(title, highlightPattern)}</div>
                <div className={styles.hitPath}>{hit.path}</div>
                <div className={styles.hitSnippet}>{highlight(hit.snippet, highlightPattern)}</div>
              </button>
            );
          })}
        </div>

        <div className={styles.footer}>
          <span>{busy ? 'searching…' : `${hits.length} match${hits.length === 1 ? '' : 'es'}`}</span>
          <span className={styles.footerHint}>
            <kbd>↵</kbd> open · <kbd>↑↓</kbd> move
          </span>
        </div>
      </div>
    </div>
  );
}

/** Split `text` around the capture group in `pattern`. When the pattern has
 * a capturing group, `String.split` interleaves matches and non-matches —
 * odd-indexed entries are always the matches, which lets us highlight
 * without re-running `test()` (which had a subtle lastIndex bug on the
 * global flag). */
function highlight(text: string, pattern: RegExp | null): React.ReactNode {
  if (!pattern) return text;
  const pieces = text.split(pattern);
  return pieces.map((p, i) =>
    i % 2 === 1 ? (
      <mark key={i} className={styles.mark}>
        {p}
      </mark>
    ) : (
      <span key={i}>{p}</span>
    ),
  );
}

function escapeRegex(s: string): string {
  return s.replace(/[.*+?^${}()|[\]\\]/g, '\\$&');
}
