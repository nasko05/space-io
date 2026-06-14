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

  useEffect(() => {
    if (open) {
      setQuery('');
      setHits([]);
      setActiveIndex(0);
      const timer = window.setTimeout(() => inputRef.current?.focus(), 0);
      return () => window.clearTimeout(timer);
    }
    return undefined;
  }, [open]);

  useEffect(() => {
    if (!open) { return; }
    const trimmed = query.trim();
    if (!trimmed) {
      setHits([]);
      return;
    }
    let cancelled = false;
    const timer = window.setTimeout(async () => {
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
        if (!cancelled) { setBusy(false); }
      }
    }, 180);
    return () => {
      cancelled = true;
      window.clearTimeout(timer);
    };
  }, [query, open]);

  function onKey(event: React.KeyboardEvent) {
    if (event.key === 'Escape') {
      event.preventDefault();
      onClose();
      return;
    }
    if (hits.length === 0) { return; }
    if (event.key === 'ArrowDown') {
      event.preventDefault();
      setActiveIndex((index) => Math.min(hits.length - 1, index + 1));
    } else if (event.key === 'ArrowUp') {
      event.preventDefault();
      setActiveIndex((index) => Math.max(0, index - 1));
    } else if (event.key === 'Enter') {
      event.preventDefault();
      const hit = hits[activeIndex];
      if (hit) {
        onSelect(hit.path);
      }
    }
  }

  const highlightPattern = useMemo(() => {
    const tokens = query
      .trim()
      .split(/\s+/)
      .filter((token) => token.length > 0);
    if (tokens.length === 0) { return null; }
    return new RegExp(`(${tokens.map(escapeRegex).join('|')})`, 'gi');
  }, [query]);

  if (!open) { return null; }

  return (
    <div className={styles.scrim} onMouseDown={onClose}>
      <div className={styles.panel} onMouseDown={(event) => event.stopPropagation()}>
        <div className={styles.inputRow}>
          <Search size={16} />
          <input
            ref={inputRef}
            className={styles.input}
            value={query}
            onChange={(event: ChangeEvent<HTMLInputElement>) => setQuery(event.target.value)}
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

/** Split `text` around the capture group in `pattern`. A capturing group makes
 * `String.split` interleave matches and non-matches, so odd-indexed pieces are
 * the matches to wrap in `<mark>`. */
function highlight(text: string, pattern: RegExp | null): React.ReactNode {
  if (!pattern) { return text; }
  const pieces = text.split(pattern);
  return pieces.map((piece, i) =>
    i % 2 === 1 ? (
      <mark key={i} className={styles.mark}>
        {piece}
      </mark>
    ) : (
      <span key={i}>{piece}</span>
    ),
  );
}

function escapeRegex(value: string): string {
  return value.replace(/[.*+?^${}()|[\]\\]/g, '\\$&');
}
