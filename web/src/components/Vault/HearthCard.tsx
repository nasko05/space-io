import { FileText, FilePdf, FileDocx, Image as ImageIcon, Video, Play } from '../icons/Icon';
import { ExcerptItem, TreeFile } from '../../api/client';
import styles from './HearthCard.module.css';

interface Props {
  file: TreeFile;
  excerpt?: ExcerptItem;
  onOpen: () => void;
}

// Ported from dir-1-hearth.jsx:771-862. Phase 2 only renders the .md card
// path with real data; .pdf/.docx/media variants are kept for when uploads
// land.
export function HearthCard({ file, excerpt, onOpen }: Props) {
  const when = formatWhen(file.updated);

  if (file.kind === 'md') {
    const title = excerpt?.title ?? file.name.replace(/\.(md|markdown)$/i, '');
    const body = excerpt?.excerpt ?? '';
    return (
      <button type="button" className={styles.mdCard} onClick={onOpen}>
        <div className={styles.mdMeta}>
          <FileText size={10} /> {when}
        </div>
        <div className={styles.mdTitle}>{title}</div>
        <div className={styles.mdExcerpt}>{body || 'empty note'}</div>
      </button>
    );
  }

  if (file.kind === 'pdf' || file.kind === 'docx') {
    const isPdf = file.kind === 'pdf';
    return (
      <button type="button" className={styles.docCard} onClick={onOpen}>
        <div className={styles.docPreview}>
          {[0.85, 0.6, 0.95, 0.55, 0.8, 0.5].map((w, i) => (
            <span key={i} className={styles.docLine} style={{ width: `${w * 100}%` }} />
          ))}
          <div className={`${styles.docBadge} ${isPdf ? styles.docBadgePdf : styles.docBadgeDocx}`}>
            {file.kind.toUpperCase()}
          </div>
        </div>
        <div className={styles.docTitle}>{file.name}</div>
        <div className={styles.docFoot}>
          <span>{when}</span>
          <span>{formatSize(file.size)}</span>
        </div>
        <div className={styles.docIcon}>
          {isPdf ? <FilePdf size={14} /> : <FileDocx size={14} />}
        </div>
      </button>
    );
  }

  // image / video / other
  const isVideo = file.kind === 'video';
  return (
    <button type="button" className={styles.mediaCard} onClick={onOpen}>
      <div className={styles.mediaTint}>
        {isVideo && (
          <div className={styles.mediaPlay}>
            <Play size={14} />
          </div>
        )}
        <div className={styles.mediaKind}>
          {isVideo ? <Video size={11} /> : <ImageIcon size={11} />} {file.kind}
        </div>
      </div>
      <div className={styles.mediaFoot}>
        <div className={styles.mediaTitle}>{file.name}</div>
        <div className={styles.mediaMeta}>
          <span>{when}</span>
          <span>{formatSize(file.size)}</span>
        </div>
      </div>
    </button>
  );
}

function formatWhen(iso: string): string {
  if (!iso) return '';
  const date = new Date(iso);
  const now = new Date();
  const diff = (now.getTime() - date.getTime()) / 1000;
  if (diff < 60) return 'just now';
  if (diff < 3600) return `${Math.round(diff / 60)} min ago`;
  if (diff < 86400) return `${Math.round(diff / 3600)}h ago`;
  if (diff < 86400 * 2) return 'yesterday';
  if (diff < 86400 * 7) return `${Math.round(diff / 86400)}d ago`;
  return date.toLocaleDateString(undefined, { month: 'short', day: 'numeric' });
}

function formatSize(bytes: number): string {
  if (bytes < 1024) return `${bytes} B`;
  if (bytes < 1024 * 1024) return `${(bytes / 1024).toFixed(1)} KB`;
  return `${(bytes / 1024 / 1024).toFixed(1)} MB`;
}
