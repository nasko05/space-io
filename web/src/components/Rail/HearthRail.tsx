import { HardDrive, Pencil } from '../icons/Icon';
import styles from './HearthRail.module.css';

// Visual-only mock data for the slice — Phase 2 wires this to real data.
const TODAY_ENTRIES = [
  {
    time: '08:14',
    title: 'Sunday morning, 27 May',
    current: true,
  },
  { time: '10:32', title: 'Idea — book club at home' },
  { time: '12:08', title: 'Lunch with Ada · scratch' },
  { time: '13:45', title: 'Sourdough — tweaked hydration' },
];

const FILLED_DAYS = new Set([3, 5, 8, 12, 13, 18, 20, 23, 26, 27]);
const TODAY = 27;
const DAYS = Array.from({ length: 31 }, (_, i) => i + 1);

// Ported from dir-1-hearth.jsx:52-153. Slice: visual only; no click handlers.
export function HearthRail() {
  return (
    <aside className={styles.rail}>
      <div className={styles.brandRow}>
        <div className={styles.brandMark}>D</div>
        <div className={styles.brandName}>SpaceIO</div>
        <div className={styles.brandLabel}>my space</div>
      </div>

      <button type="button" className={styles.newEntry}>
        <Pencil size={13} /> New entry
      </button>

      <div>
        <div className={styles.sectionLabel}>May 2026</div>
        <div className={styles.calendar}>
          {['S', 'M', 'T', 'W', 'T', 'F', 'S'].map((d, i) => (
            <div key={`h${i}`} className={styles.calHead}>
              {d}
            </div>
          ))}
          {Array.from({ length: 5 }, (_, i) => (
            <div key={`e${i}`} />
          ))}
          {DAYS.map((d) => {
            const has = FILLED_DAYS.has(d);
            const cur = d === TODAY;
            return (
              <div
                key={d}
                className={[
                  styles.calCell,
                  cur ? styles.calCellCurrent : '',
                  has && !cur ? styles.calCellHas : '',
                ]
                  .filter(Boolean)
                  .join(' ')}
              >
                {d}
                {has && !cur && <span className={styles.calDot} />}
              </div>
            );
          })}
        </div>
      </div>

      <div>
        <div className={styles.todayHead}>
          <span>Today</span>
          <span className={styles.todayRule} />
          <span className={styles.todayCount}>{TODAY_ENTRIES.length} notes</span>
        </div>
        <ul className={styles.todayList}>
          {TODAY_ENTRIES.map((it, i) => (
            <li
              key={i}
              className={[styles.todayItem, it.current ? styles.todayItemCurrent : '']
                .filter(Boolean)
                .join(' ')}
            >
              <span className={styles.todayTime}>{it.time}</span>
              <span className={styles.todayTitle}>{it.title}</span>
            </li>
          ))}
        </ul>
      </div>

      <div className={styles.storage}>
        <HardDrive size={12} />
        <div>2.4 GB used · 47.6 free</div>
      </div>
    </aside>
  );
}
