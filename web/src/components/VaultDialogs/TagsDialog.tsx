import { KeyboardEvent, useEffect, useRef, useState } from 'react';
import { Close } from '../icons/Icon';
import { useAsyncDialog } from '../../lib/useAsyncDialog';
import styles from './dialog.module.css';

interface Props {
  open: boolean;
  /** Initial tags. For multi-file selection this should be the intersection
   * of the selected files' tags (only show what they ALL have). */
  initialTags: string[];
  /** How many files the new tag set will apply to. */
  fileCount: number;
  /** All tags that exist anywhere in the space — used for suggestions. */
  knownTags: string[];
  onClose: () => void;
  onSave: (tags: string[]) => Promise<void>;
}

export function TagsDialog({
  open,
  initialTags,
  fileCount,
  knownTags,
  onClose,
  onSave,
}: Props) {
  const [tags, setTags] = useState<string[]>(initialTags);
  const [draft, setDraft] = useState('');
  const { busy, error, run } = useAsyncDialog(open, 'tag update failed');
  const inputRef = useRef<HTMLInputElement | null>(null);

  useEffect(() => {
    if (open) {
      setTags(initialTags);
      setDraft('');
      const t = window.setTimeout(() => inputRef.current?.focus(), 0);
      return () => window.clearTimeout(t);
    }
    return undefined;
  }, [open, initialTags]);

  function commitDraft() {
    const t = draft.trim();
    if (!t) return;
    if (tags.some((existing) => existing.toLowerCase() === t.toLowerCase())) {
      setDraft('');
      return;
    }
    setTags((cur) => [...cur, t]);
    setDraft('');
  }

  function removeTag(idx: number) {
    setTags((cur) => cur.filter((_, i) => i !== idx));
  }

  function onKey(e: KeyboardEvent<HTMLInputElement>) {
    if (e.key === 'Enter' || e.key === ',') {
      e.preventDefault();
      commitDraft();
    } else if (e.key === 'Backspace' && !draft && tags.length > 0) {
      removeTag(tags.length - 1);
    }
  }

  async function save() {
    // Commit any in-progress draft into the tag set before saving.
    const finalTags = (() => {
      const t = draft.trim();
      if (!t) return tags;
      if (tags.some((x) => x.toLowerCase() === t.toLowerCase())) return tags;
      return [...tags, t];
    })();
    await run(() => onSave(finalTags), { onSuccess: onClose });
  }

  if (!open) return null;

  const suggestions = knownTags.filter(
    (k) => !tags.some((t) => t.toLowerCase() === k.toLowerCase()),
  );

  return (
    <div className={styles.scrim} onMouseDown={onClose}>
      <div className={styles.panel} onMouseDown={(e) => e.stopPropagation()}>
        <div className={styles.header}>
          <div>
            <h2 className={styles.title}>Tags</h2>
            <div className={styles.subtitle}>
              {fileCount === 1
                ? 'Add or remove tags for this file.'
                : `Will replace tags on ${fileCount} files.`}
            </div>
          </div>
          <button type="button" className={styles.close} onClick={onClose} aria-label="Close">
            <Close size={14} />
          </button>
        </div>

        <div className={styles.chipRow}>
          {tags.map((t, i) => (
            <span key={`${t}-${i}`} className={styles.chip}>
              {t}
              <button
                type="button"
                className={styles.chipRemove}
                onClick={() => removeTag(i)}
                aria-label={`Remove ${t}`}
              >
                ×
              </button>
            </span>
          ))}
          <input
            ref={inputRef}
            className={styles.chipInput}
            value={draft}
            onChange={(e) => setDraft(e.target.value)}
            onKeyDown={onKey}
            onBlur={commitDraft}
            placeholder={tags.length === 0 ? 'Add a tag…' : ''}
          />
        </div>

        {suggestions.length > 0 && (
          <div>
            <div className={styles.label}>Existing tags</div>
            <div className={styles.chipRow} style={{ minHeight: 0 }}>
              {suggestions.slice(0, 24).map((t) => (
                <button
                  key={t}
                  type="button"
                  className={styles.chip}
                  style={{ cursor: 'pointer' }}
                  onClick={() => setTags((cur) => [...cur, t])}
                >
                  + {t}
                </button>
              ))}
            </div>
          </div>
        )}

        {error && <div className={styles.error} role="alert">{error}</div>}

        <div className={styles.actions}>
          <button type="button" className={styles.cancelBtn} onClick={onClose}>
            Cancel
          </button>
          <button type="button" className={styles.submitBtn} onClick={save} disabled={busy}>
            {busy ? 'Saving…' : 'Save tags'}
          </button>
        </div>
      </div>
    </div>
  );
}
