import { FolderOpen, HardDrive, Pencil } from '../icons/Icon';
import { CalendarView, TodayEntry } from '../../lib/calendar';
import styles from './HearthRail.module.css';

interface Props {
  calendar: CalendarView;
  today: TodayEntry[];
  onNewEntry: () => void;
  onSelectFile: (path: string) => void;
  onSelectDay?: (day: number) => void;
  onOpenVault: () => void;
  onOpenPasskey?: () => void;
  hasPasskey?: boolean;
  activeSurface: 'reader' | 'vault' | 'new';
}

const WEEKDAYS = ['S', 'M', 'T', 'W', 'T', 'F', 'S'];

// Ported from dir-1-hearth.jsx:52-153. Phase 2 wires real tree data:
// - Calendar dots reflect days with .md updates in the current month.
// - Today list shows files updated today, sorted newest first.
// - "my space" badge in the brand row navigates to the Vault surface.
export function HearthRail({
  calendar,
  today,
  onNewEntry,
  onSelectFile,
  onSelectDay,
  onOpenVault,
  onOpenPasskey,
  hasPasskey,
  activeSurface,
}: Props) {
  const days = Array.from({ length: calendar.daysInMonth }, (_, i) => i + 1);
  return (
    <aside className={styles.rail}>
      <div className={styles.brandRow}>
        <div className={styles.brandMark}>D</div>
        <div className={styles.brandName}>SpaceIO</div>
        <button
          type="button"
          className={`${styles.brandLabel} ${activeSurface === 'vault' ? styles.brandLabelActive : ''}`}
          onClick={onOpenVault}
        >
          my space
        </button>
      </div>

      <div className={styles.primaryActions}>
        <button type="button" className={styles.newEntry} onClick={onNewEntry}>
          <Pencil size={13} /> New entry
        </button>
        <button
          type="button"
          className={`${styles.vaultBtn} ${activeSurface === 'vault' ? styles.vaultBtnActive : ''}`}
          onClick={onOpenVault}
          title="Browse everything in this space"
        >
          <FolderOpen size={13} /> My space
        </button>
      </div>

      <div>
        <div className={styles.sectionLabel}>{calendar.monthLabel}</div>
        <div className={styles.calendar}>
          {WEEKDAYS.map((d, i) => (
            <div key={`h${i}`} className={styles.calHead}>
              {d}
            </div>
          ))}
          {Array.from({ length: calendar.startWeekday }, (_, i) => (
            <div key={`e${i}`} />
          ))}
          {days.map((d) => {
            const has = calendar.filled.has(d);
            const cur = d === calendar.today;
            const clickable = !!onSelectDay;
            const className = [
              styles.calCell,
              cur ? styles.calCellCurrent : '',
              has && !cur ? styles.calCellHas : '',
              clickable ? styles.calCellClickable : '',
            ]
              .filter(Boolean)
              .join(' ');
            return (
              <button
                key={d}
                type="button"
                className={className}
                onClick={clickable ? () => onSelectDay?.(d) : undefined}
                disabled={!clickable}
                aria-label={
                  clickable
                    ? has
                      ? `Notes from day ${d}`
                      : `Write a note for day ${d}`
                    : undefined
                }
              >
                {d}
                {has && !cur && <span className={styles.calDot} />}
              </button>
            );
          })}
        </div>
      </div>

      <div>
        <div className={styles.todayHead}>
          <span>Today</span>
          <span className={styles.todayRule} />
          <span className={styles.todayCount}>
            {today.length === 0
              ? 'nothing yet'
              : `${today.length} note${today.length === 1 ? '' : 's'}`}
          </span>
        </div>
        {today.length === 0 ? (
          <div className={styles.todayEmpty}>
            Begin where you are. Press <em>New entry</em>.
          </div>
        ) : (
          <ul className={styles.todayList}>
            {today.map((it) => (
              <li
                key={it.path}
                className={[
                  styles.todayItem,
                  it.current ? styles.todayItemCurrent : '',
                ]
                  .filter(Boolean)
                  .join(' ')}
              >
                <button
                  type="button"
                  className={styles.todayBtn}
                  onClick={() => onSelectFile(it.path)}
                >
                  <span className={styles.todayTime}>{it.time}</span>
                  <span className={styles.todayTitle}>{it.title}</span>
                </button>
              </li>
            ))}
          </ul>
        )}
      </div>

      <div className={styles.storage}>
        <HardDrive size={12} />
        <div>self-hosted · encrypted</div>
      </div>
      {onOpenPasskey && (
        <button type="button" className={styles.passkeyLink} onClick={onOpenPasskey}>
          <span className={`${styles.passkeyDot} ${hasPasskey ? styles.passkeyDotOn : ''}`} />
          {hasPasskey ? 'passkey active' : 'register a passkey →'}
        </button>
      )}
    </aside>
  );
}
