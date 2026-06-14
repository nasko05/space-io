import {
  DragEvent,
  MouseEvent as ReactMouseEvent,
  ReactNode,
  memo,
  useMemo,
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
  onOpen: (file: TreeFile) => void;
  onContextMenu?: (file: TreeFile, x: number, y: number) => void;
  onToggleSelect?: (file: TreeFile, mods: { shift?: boolean; cmd?: boolean }) => void;
}

const DRAG_MIME = 'application/x-hearth-path';

function HearthCardImpl({
  file,
  excerpt,
  tags,
  selected = false,
  onOpen,
  onContextMenu,
  onToggleSelect,
}: Props) {
  function handleContextMenu(event: ReactMouseEvent<HTMLElement>) {
    if (!onContextMenu) return;
    event.preventDefault();
    onContextMenu(file, event.clientX, event.clientY);
  }

  function onCheckboxClick(event: ReactMouseEvent<HTMLButtonElement>) {
    event.stopPropagation();
    if (!onToggleSelect) return;
    onToggleSelect(file, {
      shift: event.shiftKey,
      cmd: event.metaKey || event.ctrlKey,
    });
  }

  function onCardClick() {
    onOpen(file);
  }

  function onDragStart(event: DragEvent<HTMLElement>) {
    event.dataTransfer.effectAllowed = 'move';
    event.dataTransfer.setData(DRAG_MIME, file.path);
    event.dataTransfer.setData('text/plain', file.path);
  }

  const when = useMemo(() => formatWhen(file.updated), [file.updated]);

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
        {tags.slice(0, 3).map((tag) => (
          <span key={tag} className={styles.tag}>
            #{tag}
          </span>
        ))}
      </div>
    ) : null;

  const rootClass = `${styles.cardWrap} ${selected ? styles.cardWrapSelected : ''}`;

  if (file.kind === 'md') {
    const title = excerpt?.title ?? stripMarkdownExt(file.name);
    const body = excerpt?.excerpt ?? '';
    return (
      <div className={rootClass}>
        {overlay}
        <CardSurface
          className={styles.mdCard}
          onClick={onCardClick}
          handleContextMenu={handleContextMenu}
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
          onClick={onCardClick}
          handleContextMenu={handleContextMenu}
          onDragStart={onDragStart}
        >
          <div className={styles.docPreview}>
            {DOC_LINE_WIDTHS.map((width, i) => (
              <span key={i} className={styles.docLine} style={{ width: `${width}%` }} />
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

  const isVideo = file.kind === 'video';
  return (
    <div className={rootClass}>
      {overlay}
      <CardSurface
        className={styles.mediaCard}
        onClick={onCardClick}
        handleContextMenu={handleContextMenu}
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

/** Memoized so a selection toggle or drag highlight re-renders only the affected
 *  card, not every visible card in the vault. Callers must keep its callback
 *  props referentially stable for the memo to hold. */
export const HearthCard = memo(HearthCardImpl);

interface SurfaceProps {
  className: string;
  children: ReactNode;
  onClick: () => void;
  handleContextMenu: (event: ReactMouseEvent<HTMLElement>) => void;
  onDragStart: (event: DragEvent<HTMLElement>) => void;
}

function CardSurface({ className, children, onClick, handleContextMenu, onDragStart }: SurfaceProps) {
  return (
    <div
      role="button"
      tabIndex={0}
      className={className}
      onClick={onClick}
      onKeyDown={(event) => {
        if (event.key === 'Enter' || event.key === ' ') {
          event.preventDefault();
          onClick();
        }
      }}
      onContextMenu={handleContextMenu}
      draggable
      onDragStart={onDragStart}
    >
      {children}
    </div>
  );
}

const DOC_LINE_WIDTHS = [85, 60, 95, 55, 80, 50];

function stripMarkdownExt(name: string): string {
  return name.replace(/\.(md|markdown)$/i, '');
}

const SEC = 1000;
const MIN = 60 * SEC;
const HOUR = 60 * MIN;
const DAY = 24 * HOUR;

function formatWhen(iso: string): string {
  if (!iso) return '';
  const timestamp = Date.parse(iso);
  if (!Number.isFinite(timestamp)) return '';
  const diff = Date.now() - timestamp;
  if (diff < MIN) return 'just now';
  if (diff < HOUR) return `${Math.round(diff / MIN)} min ago`;
  if (diff < DAY) return `${Math.round(diff / HOUR)}h ago`;
  if (diff < 2 * DAY) return 'yesterday';
  if (diff < 7 * DAY) return `${Math.round(diff / DAY)}d ago`;
  return new Date(timestamp).toLocaleDateString(undefined, { month: 'short', day: 'numeric' });
}
