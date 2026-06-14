import { useEffect, useState } from 'react';
import { api, TreeFile } from '../../api/client';
import { Close, Download as DownloadIcon, Folder } from '../icons/Icon';
import { formatSize } from '../../lib/format';
import styles from './DownloadModal.module.css';

interface Props {
  open: boolean;
  file: TreeFile | null;
  onClose: () => void;
}

type Phase = { kind: 'ready' } | { kind: 'fetching'; progress: number } | { kind: 'done' };

/**
 * The format chooser and "keep a copy" toggle are visual only — the backend
 * serves the original format, and the canonical copy already lives in the space.
 */
export function DownloadModal({ open, file, onClose }: Props) {
  const [format, setFormat] = useState<'original' | 'archival' | 'print'>('original');
  const [phase, setPhase] = useState<Phase>({ kind: 'ready' });
  const [keepCopy, setKeepCopy] = useState(true);

  useEffect(() => {
    if (open) {
      setFormat('original');
      setPhase({ kind: 'ready' });
      setKeepCopy(true);
    }
  }, [open]);

  if (!open || !file) return null;

  async function save() {
    if (!file) return;
    setPhase({ kind: 'fetching', progress: 0 });
    try {
      const url = api.downloadUrl(file.path);
      const blob = await fetchWithProgress(url, (loaded, total) => {
        setPhase({ kind: 'fetching', progress: total ? loaded / total : 0 });
      });
      const fileName = file.path.split('/').pop() ?? file.path;
      const objectUrl = URL.createObjectURL(blob);
      const anchor = document.createElement('a');
      anchor.href = objectUrl;
      anchor.download = fileName;
      document.body.appendChild(anchor);
      anchor.click();
      anchor.remove();
      URL.revokeObjectURL(objectUrl);
      setPhase({ kind: 'done' });
      window.setTimeout(onClose, 500);
    } catch (err) {
      console.error('download failed', err);
      setPhase({ kind: 'ready' });
    }
  }

  const kind = file.kind === 'pdf' ? 'PDF' : file.kind === 'docx' ? 'DOC' : file.kind.toUpperCase();
  const fileName = file.path.split('/').pop() ?? file.path;
  const parent = file.path.split('/').slice(0, -1).join(' / ') || 'space';
  const fetching = phase.kind === 'fetching';
  const progressPct = phase.kind === 'fetching' ? Math.round(phase.progress * 100) : 0;

  return (
    <div className={styles.scrim} onMouseDown={onClose}>
      <div className={styles.panel} onMouseDown={(event) => event.stopPropagation()}>
        <div className={styles.top}>
          <div className={styles.thumb}>
            {[0.85, 0.6, 0.95, 0.55, 0.8, 0.5].map((width, i) => (
              <span key={i} className={styles.thumbLine} style={{ width: `${width * 100}%` }} />
            ))}
            <div className={styles.thumbBadge}>{kind}</div>
          </div>
          <div className={styles.head}>
            <div className={styles.headTitle}>{fileName}</div>
            <div className={styles.headSub}>
              {parent} · {formatSize(file.size)} · added{' '}
              {file.updated ? new Date(file.updated).toLocaleDateString() : 'unknown'}
            </div>
          </div>
          <button className={styles.close} onClick={onClose} aria-label="Close">
            <Close size={15} />
          </button>
        </div>

        <div className={styles.progress}>
          <div className={styles.progressHead}>
            <div className={styles.progressLabel}>
              {phase.kind === 'done'
                ? 'Saved to disk'
                : phase.kind === 'fetching'
                ? 'Preparing for export…'
                : 'Ready to export'}
            </div>
            <div className={styles.progressPct}>
              {phase.kind === 'fetching' && `${progressPct}%`}
              {phase.kind === 'done' && '100%'}
              {phase.kind === 'ready' && `${formatSize(file.size)}`}
            </div>
          </div>
          <div className={styles.bar}>
            <div
              className={styles.barFill}
              style={{ width: `${phase.kind === 'done' ? 100 : progressPct}%` }}
            />
          </div>
          <div className={styles.progressFoot}>
            <span>Decrypting from home server</span>
            <span>
              {phase.kind === 'done'
                ? 'done'
                : phase.kind === 'fetching'
                ? 'in progress…'
                : 'press save to begin'}
            </span>
          </div>
        </div>

        <div className={styles.sectionLabel}>Take it as</div>
        <div className={styles.formats}>
          {(
            [
              { id: 'original', label: 'Original', sub: file.kind === 'md' ? 'markdown' : 'as stored' },
              { id: 'archival', label: 'PDF/A', sub: 'archival' },
              { id: 'print', label: 'Print', sub: 'open in viewer' },
            ] as const
          ).map((option) => (
            <button
              key={option.id}
              type="button"
              className={`${styles.format} ${format === option.id ? styles.formatActive : ''}`}
              onClick={() => setFormat(option.id)}
              disabled={option.id !== 'original'}
              title={option.id !== 'original' ? 'coming soon' : undefined}
            >
              <div className={styles.formatLabel}>{option.label}</div>
              <div className={styles.formatSub}>{option.sub}</div>
            </button>
          ))}
        </div>

        <div className={styles.saveRow}>
          <span className={styles.saveLabel}>Save to</span>
          <span className={styles.saveTarget}>
            <Folder size={11} /> Downloads
          </span>
          <label className={styles.keep}>
            <span className={`${styles.checkbox} ${keepCopy ? styles.checkboxOn : ''}`}>
              {keepCopy && '✓'}
            </span>
            <input
              type="checkbox"
              className={styles.keepInput}
              checked={keepCopy}
              onChange={(event) => setKeepCopy(event.target.checked)}
            />
            Keep a copy in my space
          </label>
        </div>

        <div className={styles.footer}>
          <button type="button" className={styles.cancel} onClick={onClose}>
            Cancel
          </button>
          <button
            type="button"
            className={styles.submit}
            onClick={save}
            disabled={fetching}
          >
            <DownloadIcon size={13} /> {fetching ? 'Saving…' : 'Save to disk'}
          </button>
        </div>
      </div>
    </div>
  );
}

function fetchWithProgress(
  url: string,
  onProgress: (loaded: number, total: number) => void,
): Promise<Blob> {
  return new Promise((resolve, reject) => {
    const xhr = new XMLHttpRequest();
    xhr.open('GET', url, true);
    xhr.withCredentials = true;
    xhr.responseType = 'blob';
    xhr.onprogress = (event) => {
      if (event.lengthComputable) onProgress(event.loaded, event.total);
    };
    xhr.onload = () => {
      if (xhr.status >= 200 && xhr.status < 300) {
        resolve(xhr.response as Blob);
      } else {
        reject(new Error(`download failed (${xhr.status})`));
      }
    };
    xhr.onerror = () => reject(new Error('network error'));
    xhr.send();
  });
}

