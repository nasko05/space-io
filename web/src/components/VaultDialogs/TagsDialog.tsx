import { KeyboardEvent, useEffect, useRef, useState } from 'react';
import { DialogShell } from './DialogShell';
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
      const timer = window.setTimeout(() => inputRef.current?.focus(), 0);
      return () => window.clearTimeout(timer);
    }
    return undefined;
  }, [open, initialTags]);

  function commitDraft() {
    const trimmed = draft.trim();
    if (!trimmed) { return; }
    if (tags.some((existing) => existing.toLowerCase() === trimmed.toLowerCase())) {
      setDraft('');
      return;
    }
    setTags((cur) => [...cur, trimmed]);
    setDraft('');
  }

  function removeTag(index: number) {
    setTags((cur) => cur.filter((_, i) => i !== index));
  }

  function onKey(event: KeyboardEvent<HTMLInputElement>) {
    if (event.key === 'Enter' || event.key === ',') {
      event.preventDefault();
      commitDraft();
    } else if (event.key === 'Backspace' && !draft && tags.length > 0) {
      removeTag(tags.length - 1);
    }
  }

  async function save() {
    const finalTags = (() => {
      const trimmed = draft.trim();
      if (!trimmed) { return tags; }
      if (tags.some((existing) => existing.toLowerCase() === trimmed.toLowerCase())) { return tags; }
      return [...tags, trimmed];
    })();
    await run(() => onSave(finalTags), { onSuccess: onClose });
  }

  if (!open) { return null; }

  const suggestions = knownTags.filter(
    (candidate) => !tags.some((tag) => tag.toLowerCase() === candidate.toLowerCase()),
  );

  return (
    <DialogShell
      title="Tags"
      subtitle={
        fileCount === 1
          ? 'Add or remove tags for this file.'
          : `Will replace tags on ${fileCount} files.`
      }
      onClose={onClose}
    >
        <div className={styles.chipRow}>
          {tags.map((tag, i) => (
            <span key={`${tag}-${i}`} className={styles.chip}>
              {tag}
              <button
                type="button"
                className={styles.chipRemove}
                onClick={() => removeTag(i)}
                aria-label={`Remove ${tag}`}
              >
                ×
              </button>
            </span>
          ))}
          <input
            ref={inputRef}
            className={styles.chipInput}
            value={draft}
            onChange={(event) => setDraft(event.target.value)}
            onKeyDown={onKey}
            onBlur={commitDraft}
            placeholder={tags.length === 0 ? 'Add a tag…' : ''}
          />
        </div>

        {suggestions.length > 0 && (
          <div>
            <div className={styles.label}>Existing tags</div>
            <div className={styles.chipRow} style={{ minHeight: 0 }}>
              {suggestions.slice(0, 24).map((tag) => (
                <button
                  key={tag}
                  type="button"
                  className={styles.chip}
                  style={{ cursor: 'pointer' }}
                  onClick={() => setTags((cur) => [...cur, tag])}
                >
                  + {tag}
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
    </DialogShell>
  );
}
