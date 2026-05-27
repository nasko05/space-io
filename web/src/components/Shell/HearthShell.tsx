import { ReactNode } from 'react';
import { WindowChrome } from '../WindowChrome/WindowChrome';
import { Sun } from '../icons/Icon';
import styles from './HearthShell.module.css';

interface Props {
  children: ReactNode;
  mode?: string;
  onLock?: () => void;
}

// Ported from dir-1-hearth.jsx:26-43. The Sun icon is decorative in the slice.
export function HearthShell({ children, mode = 'reading', onLock }: Props) {
  return (
    <div className={styles.root}>
      <WindowChrome
        title="SpaceIO"
        right={
          <>
            <button type="button" className={styles.modeBtn} onClick={onLock}>
              {mode}
            </button>
            <span className={`${styles.modeBtn} ${styles.muted}`}>
              <Sun size={13} />
            </span>
          </>
        }
      />
      {children}
    </div>
  );
}
