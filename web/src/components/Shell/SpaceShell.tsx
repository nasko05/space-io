import { ReactNode } from 'react';
import { WindowChrome } from '../WindowChrome/WindowChrome';
import { Moon, Sun } from '../icons/Icon';
import styles from './SpaceShell.module.css';

interface Props {
  children: ReactNode;
  mode?: string;
  theme?: 'light' | 'dark';
  onLock?: () => void;
  onToggleTheme?: () => void;
}

export function SpaceShell({
  children,
  mode = 'reading',
  theme = 'light',
  onLock,
  onToggleTheme,
}: Props) {
  return (
    <div className={styles.root}>
      <WindowChrome
        title="SpaceIO"
        right={
          <>
            <button type="button" className={styles.modeBtn} onClick={onLock}>
              {mode}
            </button>
            <button
              type="button"
              className={`${styles.modeBtn} ${styles.muted}`}
              onClick={onToggleTheme}
              aria-label={theme === 'dark' ? 'Switch to light mode' : 'Switch to dark mode'}
              title={theme === 'dark' ? 'Switch to light mode' : 'Switch to dark mode'}
            >
              {theme === 'dark' ? <Moon size={13} /> : <Sun size={13} />}
            </button>
          </>
        }
      />
      {children}
    </div>
  );
}
