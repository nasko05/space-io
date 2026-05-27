import { ReactNode, useEffect, useLayoutEffect, useRef, useState } from 'react';
import styles from './ContextMenu.module.css';

export interface MenuItem {
  label: string;
  icon?: ReactNode;
  onClick: () => void;
  destructive?: boolean;
  disabled?: boolean;
  divider?: boolean;
}

interface Props {
  open: boolean;
  /** Viewport coords of the trigger (usually the mouse event). */
  x: number;
  y: number;
  items: MenuItem[];
  onClose: () => void;
}

/** Generic right-click menu. Positions itself at (x, y), flips to fit
 * the viewport, closes on outside click / Escape / item activation. */
export function ContextMenu({ open, x, y, items, onClose }: Props) {
  const ref = useRef<HTMLDivElement | null>(null);
  const [coords, setCoords] = useState({ left: x, top: y });

  useLayoutEffect(() => {
    if (!open) return;
    const el = ref.current;
    if (!el) return;
    const rect = el.getBoundingClientRect();
    const margin = 8;
    let left = x;
    let top = y;
    if (left + rect.width + margin > window.innerWidth) {
      left = Math.max(margin, window.innerWidth - rect.width - margin);
    }
    if (top + rect.height + margin > window.innerHeight) {
      top = Math.max(margin, window.innerHeight - rect.height - margin);
    }
    setCoords({ left, top });
  }, [open, x, y, items.length]);

  useEffect(() => {
    if (!open) return;
    function onDocMouseDown(e: MouseEvent) {
      if (!ref.current) return;
      if (!ref.current.contains(e.target as Node)) onClose();
    }
    function onKey(e: KeyboardEvent) {
      if (e.key === 'Escape') {
        e.preventDefault();
        onClose();
      }
    }
    document.addEventListener('mousedown', onDocMouseDown);
    document.addEventListener('keydown', onKey);
    return () => {
      document.removeEventListener('mousedown', onDocMouseDown);
      document.removeEventListener('keydown', onKey);
    };
  }, [open, onClose]);

  if (!open) return null;

  return (
    <div
      ref={ref}
      className={styles.menu}
      style={{ left: coords.left, top: coords.top }}
      role="menu"
      onContextMenu={(e) => e.preventDefault()}
    >
      {items.map((it, i) =>
        it.divider ? (
          <div key={`d${i}`} className={styles.divider} />
        ) : (
          <button
            key={it.label + i}
            type="button"
            role="menuitem"
            disabled={it.disabled}
            className={`${styles.item} ${it.destructive ? styles.destructive : ''}`}
            onClick={() => {
              onClose();
              it.onClick();
            }}
          >
            {it.icon && <span className={styles.icon}>{it.icon}</span>}
            <span className={styles.label}>{it.label}</span>
          </button>
        ),
      )}
    </div>
  );
}
