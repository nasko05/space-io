import { ReactNode } from 'react';
import styles from './WindowChrome.module.css';

interface Props {
  title: string;
  right?: ReactNode;
  tone?: 'light' | 'dark';
}

export function WindowChrome({ title, right, tone = 'light' }: Props) {
  return (
    <div className={tone === 'dark' ? styles.chromeDark : styles.chrome}>
      <div className={styles.lights}>
        <span style={{ background: '#ff5f57' }} />
        <span style={{ background: '#febc2e' }} />
        <span style={{ background: '#28c840' }} />
      </div>
      <div className={styles.title}>{title}</div>
      <div className={styles.right}>{right}</div>
    </div>
  );
}
