import { ExcerptMap, TreeFile, flattenFiles, TreeNode } from '../api/client';

export interface TodayEntry {
  path: string;
  title: string;
  time: string;
  current: boolean;
}

export interface CalendarView {
  year: number;
  month: number; // 0-indexed
  monthLabel: string;
  today: number;
  daysInMonth: number;
  startWeekday: number; // Sunday = 0
  filled: Set<number>; // days with at least one entry
}

const MONTHS = [
  'January',
  'February',
  'March',
  'April',
  'May',
  'June',
  'July',
  'August',
  'September',
  'October',
  'November',
  'December',
];

function pad(n: number): string {
  return n < 10 ? `0${n}` : `${n}`;
}

export function buildCalendar(now: Date, tree: TreeNode[]): CalendarView {
  const year = now.getFullYear();
  const month = now.getMonth();
  const today = now.getDate();
  const daysInMonth = new Date(year, month + 1, 0).getDate();
  const startWeekday = new Date(year, month, 1).getDay();

  const filled = new Set<number>();
  for (const f of flattenFiles(tree)) {
    if (f.kind !== 'md' || !f.updated) continue;
    const d = new Date(f.updated);
    if (d.getFullYear() === year && d.getMonth() === month) {
      filled.add(d.getDate());
    }
  }

  return {
    year,
    month,
    monthLabel: `${MONTHS[month]} ${year}`,
    today,
    daysInMonth,
    startWeekday,
    filled,
  };
}

export function entriesForToday(
  now: Date,
  tree: TreeNode[],
  excerpts: ExcerptMap,
  currentPath: string | null,
): TodayEntry[] {
  const sameDay = (a: Date, b: Date) =>
    a.getFullYear() === b.getFullYear() &&
    a.getMonth() === b.getMonth() &&
    a.getDate() === b.getDate();

  const files = flattenFiles(tree).filter((f) => f.kind === 'md' && f.updated);
  return files
    .filter((f) => sameDay(new Date(f.updated), now))
    .sort((a, b) => new Date(b.updated).getTime() - new Date(a.updated).getTime())
    .map((f) => {
      const d = new Date(f.updated);
      const title =
        excerpts[f.path]?.title ?? f.name.replace(/\.(md|markdown)$/i, '');
      return {
        path: f.path,
        title,
        time: `${pad(d.getHours())}:${pad(d.getMinutes())}`,
        current: f.path === currentPath,
      };
    });
}

/** Find the first markdown file updated on the given (year, month, day). */
export function findFileForDay(
  tree: TreeNode[],
  year: number,
  month: number,
  day: number,
): TreeFile | null {
  for (const f of flattenFiles(tree)) {
    if (f.kind !== 'md' || !f.updated) continue;
    const d = new Date(f.updated);
    if (d.getFullYear() === year && d.getMonth() === month && d.getDate() === day) {
      return f;
    }
  }
  return null;
}

/** Locale-independent "27 May 2026" — matches the dateline format used in the
 *  Reader and is safe to feed to the create-file endpoint as a title. */
export function dateTitle(year: number, month: number, day: number): string {
  return `${day} ${MONTHS[month]} ${year}`;
}
