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
    const element = ref.current;
    if (!element) return;
    const rect = element.getBoundingClientRect();
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
    function onDocMouseDown(event: MouseEvent) {
      if (!ref.current) return;
      if (!ref.current.contains(event.target as Node)) onClose();
    }
    function onKey(event: KeyboardEvent) {
      if (event.key === 'Escape') {
        event.preventDefault();
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
      onContextMenu={(event) => event.preventDefault()}
    >
      {items.map((item, i) =>
        item.divider ? (
          <div key={`d${i}`} className={styles.divider} />
        ) : (
          <button
            key={item.label + i}
            type="button"
            role="menuitem"
            disabled={item.disabled}
            className={`${styles.item} ${item.destructive ? styles.destructive : ''}`}
            onClick={() => {
              onClose();
              item.onClick();
            }}
          >
            {item.icon && <span className={styles.icon}>{item.icon}</span>}
            <span className={styles.label}>{item.label}</span>
          </button>
        ),
      )}
    </div>
  );
}
