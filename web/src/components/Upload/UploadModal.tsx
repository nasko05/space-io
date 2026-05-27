import { ChangeEvent, DragEvent, useEffect, useRef, useState } from 'react';
import { Close, FilePdf, Folder, Image as ImageIcon, Upload as UploadIcon, Video } from '../icons/Icon';
import { api } from '../../api/client';
import { topLevelFolders, TreeNode } from '../../api/client';
import styles from './UploadModal.module.css';

interface Props {
  open: boolean;
  initialFiles?: File[];
  tree: TreeNode[];
  onClose: () => void;
  onUploaded: () => void;
}

type Item = {
  file: File;
  state: 'queued' | 'doing' | 'done' | 'error';
  progress: number;
  error?: string;
};

const DEFAULT_FOLDER = 'Uploads';
const MAX_BYTES = 50 * 1024 * 1024;

// Ported from dir-1-hearth.jsx:385-512 (HearthUpload).
export function UploadModal({ open, initialFiles, tree, onClose, onUploaded }: Props) {
  const [items, setItems] = useState<Item[]>([]);
  const [folder, setFolder] = useState<string>(DEFAULT_FOLDER);
  const [submitting, setSubmitting] = useState(false);
  const [dragOver, setDragOver] = useState(false);
  const inputRef = useRef<HTMLInputElement | null>(null);

  // Seed queue when reopened.
  useEffect(() => {
    if (open) {
      setItems(
        (initialFiles ?? []).map<Item>((f) => ({ file: f, state: 'queued', progress: 0 })),
      );
      setSubmitting(false);
      setDragOver(false);
    }
  }, [open, initialFiles]);

  const folders = ['Uploads', ...topLevelFolders(tree).map((f) => f.name)];
  const dedupedFolders = Array.from(new Set(folders));

  function appendFiles(list: FileList | File[] | null) {
    if (!list) return;
    const accepted: Item[] = [];
    for (const f of Array.from(list)) {
      if (f.size > MAX_BYTES) {
        accepted.push({
          file: f,
          state: 'error',
          progress: 0,
          error: `${formatSize(f.size)} exceeds 50 MB`,
        });
      } else {
        accepted.push({ file: f, state: 'queued', progress: 0 });
      }
    }
    setItems((cur) => [...cur, ...accepted]);
  }

  function removeAt(idx: number) {
    setItems((cur) => cur.filter((_, i) => i !== idx));
  }

  async function submit() {
    const ready = items
      .map((it, idx) => ({ it, idx }))
      .filter(({ it }) => it.state === 'queued');
    if (ready.length === 0 || submitting) return;
    setSubmitting(true);
    for (const { idx } of ready) {
      // Snapshot the file reference; setItems is async.
      const file = items[idx]?.file;
      if (!file) continue;
      setItems((cur) => cur.map((it, i) => (i === idx ? { ...it, state: 'doing', progress: 0 } : it)));
      try {
        await api.upload(folder, [file], (loaded, total) => {
          setItems((cur) =>
            cur.map((it, i) => (i === idx ? { ...it, progress: total ? loaded / total : 0 } : it)),
          );
        });
        setItems((cur) =>
          cur.map((it, i) => (i === idx ? { ...it, state: 'done', progress: 1 } : it)),
        );
      } catch (err) {
        const message = err instanceof Error ? err.message : 'upload failed';
        setItems((cur) =>
          cur.map((it, i) => (i === idx ? { ...it, state: 'error', error: message } : it)),
        );
      }
    }
    setSubmitting(false);
    onUploaded();
  }

  function onDrop(e: DragEvent<HTMLDivElement>) {
    e.preventDefault();
    setDragOver(false);
    if (e.dataTransfer?.files) appendFiles(e.dataTransfer.files);
  }

  function onFileChange(e: ChangeEvent<HTMLInputElement>) {
    appendFiles(e.target.files);
    e.target.value = '';
  }

  if (!open) return null;

  const queuedCount = items.filter((it) => it.state === 'queued').length;
  const submitLabel = queuedCount > 0 ? `Save ${queuedCount} file${queuedCount === 1 ? '' : 's'}` : 'Done';

  return (
    <div className={styles.scrim} onMouseDown={onClose}>
      <div className={styles.panel} onMouseDown={(e) => e.stopPropagation()}>
        <div className={styles.header}>
          <h2 className={styles.title}>Bring something in</h2>
          <button type="button" className={styles.close} onClick={onClose} aria-label="Close">
            <Close size={16} />
          </button>
        </div>

        <div
          className={`${styles.dropzone} ${dragOver ? styles.dropzoneOver : ''}`}
          onDragEnter={(e) => {
            e.preventDefault();
            setDragOver(true);
          }}
          onDragOver={(e) => {
            e.preventDefault();
            setDragOver(true);
          }}
          onDragLeave={() => setDragOver(false)}
          onDrop={onDrop}
        >
          <div className={styles.dropIcon}>
            <UploadIcon size={24} />
          </div>
          <div className={styles.dropTitle}>Drop files here</div>
          <div className={styles.dropSub}>
            or{' '}
            <button type="button" className={styles.browse} onClick={() => inputRef.current?.click()}>
              browse from your computer
            </button>
          </div>
          <input
            ref={inputRef}
            type="file"
            multiple
            className={styles.fileInput}
            onChange={onFileChange}
          />
          <div className={styles.dropHint}>
            <span>.md .docx .pdf</span>
            <span>.jpg .png</span>
            <span>.mp4 up to 50 MB</span>
          </div>
        </div>

        <div className={styles.destRow}>
          <span className={styles.destLabel}>Destination</span>
          <div className={styles.destSelect}>
            <Folder size={12} />
            <select
              className={styles.destSelectInner}
              value={folder}
              onChange={(e) => setFolder(e.target.value)}
            >
              {dedupedFolders.map((f) => (
                <option key={f} value={f}>
                  {f}
                </option>
              ))}
            </select>
          </div>
        </div>

        {items.length > 0 && (
          <div className={styles.list}>
            {items.map((it, i) => (
              <div key={i} className={styles.row}>
                <div className={styles.rowIcon}>
                  {kindIcon(it.file.name)}
                </div>
                <div className={styles.rowMain}>
                  <div className={styles.rowName}>{it.file.name}</div>
                  <div className={styles.rowMeta}>
                    {it.state === 'done' && `${formatSize(it.file.size)} · saved`}
                    {it.state === 'doing' && `${formatSize(it.file.size)} · ${Math.round(it.progress * 100)}%`}
                    {it.state === 'queued' && `${formatSize(it.file.size)} · waiting`}
                    {it.state === 'error' && (
                      <span className={styles.rowError}>{it.error ?? 'failed'}</span>
                    )}
                  </div>
                  {it.state === 'doing' && (
                    <div className={styles.progressTrack}>
                      <div
                        className={styles.progressFill}
                        style={{ width: `${Math.round(it.progress * 100)}%` }}
                      />
                    </div>
                  )}
                </div>
                <div className={styles.rowState}>
                  {it.state === 'done' && '✓ Kept'}
                  {it.state === 'doing' && 'Saving'}
                  {it.state === 'queued' && (
                    <button
                      type="button"
                      className={styles.rowRemove}
                      onClick={() => removeAt(i)}
                      aria-label="Remove"
                    >
                      <Close size={12} />
                    </button>
                  )}
                  {it.state === 'error' && 'Failed'}
                </div>
              </div>
            ))}
          </div>
        )}

        <div className={styles.footer}>
          <button type="button" className={styles.cancel} onClick={onClose}>
            Cancel
          </button>
          <button
            type="button"
            className={styles.submit}
            onClick={queuedCount > 0 ? submit : onClose}
            disabled={submitting}
          >
            {submitting ? 'Saving…' : submitLabel}
          </button>
        </div>
      </div>
    </div>
  );
}

function kindIcon(name: string) {
  const ext = name.split('.').pop()?.toLowerCase() ?? '';
  if (['jpg', 'jpeg', 'png', 'gif', 'webp'].includes(ext)) return <ImageIcon size={16} />;
  if (ext === 'pdf') return <FilePdf size={16} />;
  if (['mp4', 'mov', 'webm'].includes(ext)) return <Video size={16} />;
  return <ImageIcon size={16} />;
}

function formatSize(bytes: number): string {
  if (bytes < 1024) return `${bytes} B`;
  if (bytes < 1024 * 1024) return `${(bytes / 1024).toFixed(1)} KB`;
  return `${(bytes / 1024 / 1024).toFixed(1)} MB`;
}
