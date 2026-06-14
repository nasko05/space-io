import { ChangeEvent, DragEvent, useEffect, useRef, useState } from 'react';
import { Close, FilePdf, Folder, Image as ImageIcon, Upload as UploadIcon, Video } from '../icons/Icon';
import { api } from '../../api/client';
import { topLevelFolders, TreeNode } from '../../api/client';
import { formatSize, shortId } from '../../lib/format';
import styles from './UploadModal.module.css';

interface Props {
  open: boolean;
  initialFiles?: File[];
  tree: TreeNode[];
  onClose: () => void;
  onUploaded: () => void;
}

type Item = {
  id: string;
  file: File;
  state: 'queued' | 'doing' | 'done' | 'error';
  progress: number;
  error?: string;
};

const DEFAULT_FOLDER = 'Uploads';
const MAX_BYTES = 50 * 1024 * 1024;

export function UploadModal({ open, initialFiles, tree, onClose, onUploaded }: Props) {
  const [items, setItems] = useState<Item[]>([]);
  const [folder, setFolder] = useState<string>(DEFAULT_FOLDER);
  const [submitting, setSubmitting] = useState(false);
  const [dragOver, setDragOver] = useState(false);
  const inputRef = useRef<HTMLInputElement | null>(null);

  useEffect(() => {
    if (open) {
      setItems(
        (initialFiles ?? []).map<Item>((file) => ({
          id: shortId('up'),
          file,
          state: 'queued',
          progress: 0,
        })),
      );
      setSubmitting(false);
      setDragOver(false);
    }
  }, [open, initialFiles]);

  const folders = ['Uploads', ...topLevelFolders(tree).map((folder) => folder.name)];
  const dedupedFolders = Array.from(new Set(folders));

  function appendFiles(list: FileList | File[] | null) {
    if (!list) return;
    const accepted: Item[] = [];
    for (const file of Array.from(list)) {
      const id = shortId('up');
      if (file.size > MAX_BYTES) {
        accepted.push({
          id,
          file,
          state: 'error',
          progress: 0,
          error: `${formatSize(file.size)} exceeds 50 MB`,
        });
      } else {
        accepted.push({ id, file, state: 'queued', progress: 0 });
      }
    }
    setItems((cur) => [...cur, ...accepted]);
  }

  function removeById(id: string) {
    setItems((cur) => cur.filter((item) => item.id !== id));
  }

  async function submit() {
    const ready = items.filter((item) => item.state === 'queued');
    if (ready.length === 0 || submitting) return;
    setSubmitting(true);
    for (const { id, file } of ready) {
      setItems((cur) =>
        cur.map((item) => (item.id === id ? { ...item, state: 'doing', progress: 0 } : item)),
      );
      try {
        await api.upload(folder, [file], (loaded, total) => {
          setItems((cur) =>
            cur.map((item) =>
              item.id === id ? { ...item, progress: total ? loaded / total : 0 } : item,
            ),
          );
        });
        setItems((cur) =>
          cur.map((item) => (item.id === id ? { ...item, state: 'done', progress: 1 } : item)),
        );
      } catch (err) {
        const message = err instanceof Error ? err.message : 'upload failed';
        setItems((cur) =>
          cur.map((item) => (item.id === id ? { ...item, state: 'error', error: message } : item)),
        );
      }
    }
    setSubmitting(false);
    onUploaded();
  }

  function onDrop(event: DragEvent<HTMLDivElement>) {
    event.preventDefault();
    setDragOver(false);
    if (event.dataTransfer?.files) appendFiles(event.dataTransfer.files);
  }

  function onFileChange(event: ChangeEvent<HTMLInputElement>) {
    appendFiles(event.target.files);
    event.target.value = '';
  }

  if (!open) return null;

  const queuedCount = items.filter((item) => item.state === 'queued').length;
  const submitLabel = queuedCount > 0 ? `Save ${queuedCount} file${queuedCount === 1 ? '' : 's'}` : 'Done';

  return (
    <div className={styles.scrim} onMouseDown={onClose}>
      <div className={styles.panel} onMouseDown={(event) => event.stopPropagation()}>
        <div className={styles.header}>
          <h2 className={styles.title}>Bring something in</h2>
          <button type="button" className={styles.close} onClick={onClose} aria-label="Close">
            <Close size={16} />
          </button>
        </div>

        <div
          className={`${styles.dropzone} ${dragOver ? styles.dropzoneOver : ''}`}
          onDragEnter={(event) => {
            event.preventDefault();
            setDragOver(true);
          }}
          onDragOver={(event) => {
            event.preventDefault();
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
              onChange={(event) => setFolder(event.target.value)}
            >
              {dedupedFolders.map((folderName) => (
                <option key={folderName} value={folderName}>
                  {folderName}
                </option>
              ))}
            </select>
          </div>
        </div>

        {items.length > 0 && (
          <div className={styles.list}>
            {items.map((item) => (
              <div key={item.id} className={styles.row}>
                <div className={styles.rowIcon}>
                  {kindIcon(item.file.name)}
                </div>
                <div className={styles.rowMain}>
                  <div className={styles.rowName}>{item.file.name}</div>
                  <div className={styles.rowMeta}>
                    {item.state === 'done' && `${formatSize(item.file.size)} · saved`}
                    {item.state === 'doing' && `${formatSize(item.file.size)} · ${Math.round(item.progress * 100)}%`}
                    {item.state === 'queued' && `${formatSize(item.file.size)} · waiting`}
                    {item.state === 'error' && (
                      <span className={styles.rowError}>{item.error ?? 'failed'}</span>
                    )}
                  </div>
                  {item.state === 'doing' && (
                    <div className={styles.progressTrack}>
                      <div
                        className={styles.progressFill}
                        style={{ width: `${Math.round(item.progress * 100)}%` }}
                      />
                    </div>
                  )}
                </div>
                <div className={styles.rowState}>
                  {item.state === 'done' && '✓ Kept'}
                  {item.state === 'doing' && 'Saving'}
                  {item.state === 'queued' && (
                    <button
                      type="button"
                      className={styles.rowRemove}
                      onClick={() => removeById(item.id)}
                      aria-label="Remove"
                    >
                      <Close size={12} />
                    </button>
                  )}
                  {item.state === 'error' && 'Failed'}
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
