import { memo } from 'react';
import { Close, FolderOpen, HardDrive, Pencil } from '../icons/Icon';
import { CalendarView, TodayEntry } from '../../lib/calendar';
import styles from './SpaceRail.module.css';

/** Full URL of the co-hosted cloud drive. When set, a "Cloud drive" link
 *  appears in the rail; the shared SSO cookie keeps the session across the jump.
 *  Empty (the default) hides it, so the editor still works standalone. */
const DRIVE_URL = (import.meta.env.VITE_DRIVE_URL as string | undefined) ?? '';

interface Props {
  calendar: CalendarView;
  entries: TodayEntry[];
  /** Label for the section under the calendar — "Today", or a date like "27 May". */
  entriesLabel: string;
  /** When set, the calendar cell for this day is highlighted and the entries
   *  are for that day (not today). Click another day to switch; click the
   *  same day or the clear button to reset to today. */
  selectedDay?: number | null;
  onClearSelectedDay?: () => void;
  onPickDate?: (value: string) => void;
  onNewEntry: () => void;
  onSelectFile: (path: string) => void;
  onSelectDay?: (day: number) => void;
  onOpenVault: () => void;
  onOpenPasskey?: () => void;
  hasPasskey?: boolean;
  activeSurface: 'reader' | 'vault' | 'new';
}

const WEEKDAYS = ['S', 'M', 'T', 'W', 'T', 'F', 'S'];

function SpaceRailImpl({
  calendar,
  entries,
  entriesLabel,
  selectedDay = null,
  onClearSelectedDay,
  onPickDate,
  onNewEntry,
  onSelectFile,
  onSelectDay,
  onOpenVault,
  onOpenPasskey,
  hasPasskey,
  activeSurface,
}: Props) {
  const days = Array.from({ length: calendar.daysInMonth }, (_, i) => i + 1);
  const activeDay = selectedDay ?? (calendar.today || 1);
  const pad = (value: number) => String(value).padStart(2, '0');
  const dateValue = `${calendar.year}-${pad(calendar.month + 1)}-${pad(activeDay)}`;
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
        {DRIVE_URL && (
          <a className={styles.driveLink} href={DRIVE_URL} title="Open the cloud drive">
            <HardDrive size={13} /> Cloud drive
          </a>
        )}
      </div>

      <div>
        <label className={styles.calMonth}>
          <span className={styles.calMonthLabel}>{calendar.monthLabel}</span>
          {onPickDate && (
            <input
              type="date"
              className={styles.calMonthInput}
              value={dateValue}
              onChange={(event) => {
                if (event.target.value) {
                  onPickDate(event.target.value);
                }
              }}
              onClick={(event) => event.currentTarget.showPicker?.()}
              aria-label="Jump to a date"
            />
          )}
        </label>
        <div className={styles.calendar}>
          {WEEKDAYS.map((letter, i) => (
            <div key={`h${i}`} className={styles.calHead}>
              {letter}
            </div>
          ))}
          {Array.from({ length: calendar.startWeekday }, (_, i) => (
            <div key={`e${i}`} />
          ))}
          {days.map((day) => {
            const hasNotes = calendar.filled.has(day);
            const isToday = day === calendar.today;
            const isSelected = day === selectedDay;
            const isClickable = !!onSelectDay;
            const className = [
              styles.calCell,
              isToday ? styles.calCellCurrent : '',
              isSelected && !isToday ? styles.calCellSelected : '',
              hasNotes && !isToday && !isSelected ? styles.calCellHas : '',
              isClickable ? styles.calCellClickable : '',
            ]
              .filter(Boolean)
              .join(' ');
            return (
              <button
                key={day}
                type="button"
                className={className}
                onClick={isClickable ? () => onSelectDay?.(day) : undefined}
                disabled={!isClickable}
                aria-label={
                  isClickable
                    ? hasNotes
                      ? `Notes from day ${day}`
                      : `No notes on day ${day}`
                    : undefined
                }
              >
                {day}
                {hasNotes && !isToday && !isSelected && <span className={styles.calDot} />}
              </button>
            );
          })}
        </div>
      </div>

      <div className={styles.todaySection}>
        <div className={styles.todayHead}>
          <span>{entriesLabel}</span>
          {selectedDay != null && onClearSelectedDay && (
            <button
              type="button"
              className={styles.todayClear}
              onClick={onClearSelectedDay}
              title="Back to today"
              aria-label="Clear day selection"
            >
              <Close size={10} />
            </button>
          )}
          <span className={styles.todayRule} />
          <span className={styles.todayCount}>
            {entries.length === 0
              ? 'nothing yet'
              : `${entries.length} note${entries.length === 1 ? '' : 's'}`}
          </span>
        </div>
        {entries.length === 0 ? (
          <div className={styles.todayEmpty}>
            {selectedDay != null ? (
              <>No notes from that day.</>
            ) : (
              <>
                Begin where you are. Press <em>New entry</em>.
              </>
            )}
          </div>
        ) : (
          <ul className={styles.todayList}>
            {entries.map((entry) => (
              <li
                key={entry.path}
                className={[styles.todayItem, entry.current ? styles.todayItemCurrent : '']
                  .filter(Boolean)
                  .join(' ')}
              >
                <button
                  type="button"
                  className={styles.todayBtn}
                  onClick={() => onSelectFile(entry.path)}
                >
                  <span className={styles.todayTime}>{entry.time}</span>
                  <span className={styles.todayTitle}>{entry.title}</span>
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

/** Memoized so the rail doesn't re-render on every keystroke in the Reader;
 *  callers must keep callback props referentially stable for the memo to hit. */
export const SpaceRail = memo(SpaceRailImpl);
