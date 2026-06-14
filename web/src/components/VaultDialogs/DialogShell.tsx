import { FormEvent, ReactElement, ReactNode } from 'react';
import { Close } from '../icons/Icon';
import styles from './dialog.module.css';

interface Props {
  title: ReactNode;
  subtitle: ReactNode;
  onClose: () => void;
  /** When provided, the panel renders as a `<form>` and submits through this
   *  handler (e.g. RenameDialog's Enter-to-submit). Otherwise it is a plain div. */
  onSubmit?: (event: FormEvent) => void;
  children: ReactNode;
}

/** Shared chrome for the vault dialogs: the click-to-dismiss scrim, the centered
 *  panel, and the title/subtitle/close header. Each dialog supplies its own body
 *  and actions as children. */
export function DialogShell({ title, subtitle, onClose, onSubmit, children }: Props): ReactElement {
  const header = (
    <div className={styles.header}>
      <div>
        <h2 className={styles.title}>{title}</h2>
        <div className={styles.subtitle}>{subtitle}</div>
      </div>
      <button type="button" className={styles.close} onClick={onClose} aria-label="Close">
        <Close size={14} />
      </button>
    </div>
  );

  return (
    <div className={styles.scrim} onMouseDown={onClose}>
      {onSubmit ? (
        <form className={styles.panel} onMouseDown={(event) => event.stopPropagation()} onSubmit={onSubmit}>
          {header}
          {children}
        </form>
      ) : (
        <div className={styles.panel} onMouseDown={(event) => event.stopPropagation()}>
          {header}
          {children}
        </div>
      )}
    </div>
  );
}
