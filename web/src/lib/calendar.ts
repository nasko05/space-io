import { ExcerptMap, TreeFile, TreeNode } from '../api/client';

export interface TodayEntry {
  path: string;
  title: string;
  time: string;
  current: boolean;
}

export interface CalendarView {
  year: number;
  /** 0-indexed. */
  month: number;
  monthLabel: string;
  today: number;
  daysInMonth: number;
  /** Sunday = 0. */
  startWeekday: number;
  /** Days with at least one entry. */
  filled: Set<number>;
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

/** Visit every markdown file in the tree exactly once, surfacing the file
 * along with a parsed timestamp so callers don't each have to re-parse and
 * re-walk. */
function forEachMarkdown(
  tree: TreeNode[],
  visit: (file: TreeFile, ts: number) => void,
): void {
  const walk = (nodes: TreeNode[]) => {
    for (const n of nodes) {
      if (n.type === 'file') {
        if (n.kind !== 'md' || !n.updated) continue;
        const ts = Date.parse(n.updated);
        if (!Number.isFinite(ts)) continue;
        visit(n, ts);
      } else {
        walk(n.children);
      }
    }
  };
  walk(tree);
}

export function buildCalendar(view: Date, today: Date, tree: TreeNode[]): CalendarView {
  const year = view.getFullYear();
  const month = view.getMonth();
  const daysInMonth = new Date(year, month + 1, 0).getDate();
  const startWeekday = new Date(year, month, 1).getDay();
  const isCurrentMonth = today.getFullYear() === year && today.getMonth() === month;

  const filled = new Set<number>();
  forEachMarkdown(tree, (_, ts) => {
    const d = new Date(ts);
    if (d.getFullYear() === year && d.getMonth() === month) {
      filled.add(d.getDate());
    }
  });

  return {
    year,
    month,
    monthLabel: `${MONTHS[month]} ${year}`,
    today: isCurrentMonth ? today.getDate() : 0,
    daysInMonth,
    startWeekday,
    filled,
  };
}

/** Markdown notes whose `updated` timestamp falls on the same calendar day as
 *  `date`. Sorted newest first; the entry currently open in the Reader is
 *  flagged with `current: true`. */
export function entriesForDate(
  date: Date,
  tree: TreeNode[],
  excerpts: ExcerptMap,
  currentPath: string | null,
): TodayEntry[] {
  const year = date.getFullYear();
  const month = date.getMonth();
  const day = date.getDate();
  type Row = { file: TreeFile; ts: number };
  const rows: Row[] = [];
  forEachMarkdown(tree, (file, ts) => {
    const d = new Date(ts);
    if (d.getFullYear() === year && d.getMonth() === month && d.getDate() === day) {
      rows.push({ file, ts });
    }
  });
  rows.sort((a, b) => b.ts - a.ts);
  return rows.map(({ file, ts }) => {
    const d = new Date(ts);
    const title = excerpts[file.path]?.title ?? file.name.replace(/\.(md|markdown)$/i, '');
    return {
      path: file.path,
      title,
      time: `${pad(d.getHours())}:${pad(d.getMinutes())}`,
      current: file.path === currentPath,
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
  let hit: TreeFile | null = null;
  forEachMarkdown(tree, (file, ts) => {
    if (hit) return;
    const d = new Date(ts);
    if (d.getFullYear() === year && d.getMonth() === month && d.getDate() === day) {
      hit = file;
    }
  });
  return hit;
}

/** Locale-independent "27 May 2026" — matches the dateline format used in the
 *  Reader and is safe to feed to the create-file endpoint as a title. */
export function dateTitle(year: number, month: number, day: number): string {
  return `${day} ${MONTHS[month]} ${year}`;
}

/** Short "27 May" label for the calendar's day-entries header. */
export function shortDayLabel(month: number, day: number): string {
  return `${day} ${MONTHS[month]}`;
}
