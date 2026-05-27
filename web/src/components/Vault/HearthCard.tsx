import {
  DragEvent,
  MouseEvent as ReactMouseEvent,
  ReactNode,
} from 'react';
import { FileText, FilePdf, FileDocx, Image as ImageIcon, Video, Play } from '../icons/Icon';
import { ExcerptItem, TreeFile } from '../../api/client';
import { formatSize } from '../../lib/format';
import styles from './HearthCard.module.css';

interface Props {
  file: TreeFile;
  excerpt?: ExcerptItem;
  tags?: string[];
  selected?: boolean;
  onOpen: () => void;
  onContextMenu?: (x: number, y: number) => void;
  onToggleSelect?: (mods: { shift?: boolean; cmd?: boolean }) => void;
}

const DRAG_MIME = 'application/x-hearth-path';

export function HearthCard({
  file,
  excerpt,
  tags,
  selected = false,
  onOpen,
  onContextMenu,
  onToggleSelect,
}: Props) {
  function onCtx(e: ReactMouseEvent<HTMLElement>) {
    if (!onContextMenu) return;
    e.preventDefault();
    onContextMenu(e.clientX, e.clientY);
  }

  function onCheckboxClick(e: ReactMouseEvent<HTMLButtonElement>) {
    e.stopPropagation();
    if (!onToggleSelect) return;
    onToggleSelect({
      shift: e.shiftKey,
      cmd: e.metaKey || e.ctrlKey,
    });
  }

  function onDragStart(e: DragEvent<HTMLElement>) {
    e.dataTransfer.effectAllowed = 'move';
    e.dataTransfer.setData(DRAG_MIME, file.path);
    e.dataTransfer.setData('text/plain', file.path);
  }

  const overlay = (
    <button
      type="button"
      className={`${styles.checkbox} ${selected ? styles.checkboxOn : ''}`}
      onClick={onCheckboxClick}
      aria-label={selected ? 'Deselect' : 'Select'}
      title={selected ? 'Deselect' : 'Select'}
    >
      {selected ? '✓' : ''}
    </button>
  );

  const chips =
    tags && tags.length > 0 ? (
      <div className={styles.tags}>
        {tags.slice(0, 3).map((t) => (
          <span key={t} className={styles.tag}>
            #{t}
          </span>
        ))}
      </div>
    ) : null;

  const when = formatWhen(file.updated);
  const rootClass = `${styles.cardWrap} ${selected ? styles.cardWrapSelected : ''}`;

  if (file.kind === 'md') {
    const title = excerpt?.title ?? file.name.replace(/\.(md|markdown)$/i, '');
    const body = excerpt?.excerpt ?? '';
    return (
      <div className={rootClass}>
        {overlay}
        <CardSurface
          className={styles.mdCard}
          onOpen={onOpen}
          onCtx={onCtx}
          onDragStart={onDragStart}
        >
          <div className={styles.mdMeta}>
            <FileText size={10} /> {when}
          </div>
          <div className={styles.mdTitle}>{title}</div>
          <div className={styles.mdExcerpt}>{body || 'empty note'}</div>
          {chips}
        </CardSurface>
      </div>
    );
  }

  if (file.kind === 'pdf' || file.kind === 'docx') {
    const isPdf = file.kind === 'pdf';
    return (
      <div className={rootClass}>
        {overlay}
        <CardSurface
          className={styles.docCard}
          onOpen={onOpen}
          onCtx={onCtx}
          onDragStart={onDragStart}
        >
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
          {chips}
          <div className={styles.docIcon}>
            {isPdf ? <FilePdf size={14} /> : <FileDocx size={14} />}
          </div>
        </CardSurface>
      </div>
    );
  }

  // image / video / other
  const isVideo = file.kind === 'video';
  return (
    <div className={rootClass}>
      {overlay}
      <CardSurface
        className={styles.mediaCard}
        onOpen={onOpen}
        onCtx={onCtx}
        onDragStart={onDragStart}
      >
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
          {chips}
        </div>
      </CardSurface>
    </div>
  );
}

interface SurfaceProps {
  className: string;
  children: ReactNode;
  onOpen: () => void;
  onCtx: (e: ReactMouseEvent<HTMLElement>) => void;
  onDragStart: (e: DragEvent<HTMLElement>) => void;
}

function CardSurface({ className, children, onOpen, onCtx, onDragStart }: SurfaceProps) {
  return (
    <div
      role="button"
      tabIndex={0}
      className={className}
      onClick={onOpen}
      onKeyDown={(e) => {
        if (e.key === 'Enter' || e.key === ' ') {
          e.preventDefault();
          onOpen();
        }
      }}
      onContextMenu={onCtx}
      draggable
      onDragStart={onDragStart}
    >
      {children}
    </div>
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

