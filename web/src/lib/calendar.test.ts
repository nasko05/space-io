import { describe, it, expect } from 'vitest';
import { buildCalendar, entriesForDate, shortDayLabel } from './calendar';
import { TreeNode } from '../api/client';

function mdFile(path: string, updated: string): TreeNode {
  return {
    type: 'file',
    name: path.split('/').pop()!,
    path,
    kind: 'md',
    updated,
    size: 100,
  };
}

describe('buildCalendar', () => {
  it('computes correct month metadata', () => {
    const may27_2026 = new Date(2026, 4, 27);
    const cal = buildCalendar(may27_2026, may27_2026, []);
    expect(cal.year).toBe(2026);
    expect(cal.month).toBe(4);
    expect(cal.today).toBe(27);
    expect(cal.daysInMonth).toBe(31);
    expect(cal.monthLabel).toBe('May 2026');
  });

  it('marks days with updated files as filled', () => {
    const now = new Date(2026, 4, 15);
    const tree: TreeNode[] = [
      mdFile('a.md', '2026-05-10T08:00:00Z'),
      mdFile('b.md', '2026-05-15T12:00:00Z'),
      mdFile('c.md', '2026-04-10T08:00:00Z'),
    ];
    const cal = buildCalendar(now, now, tree);
    expect(cal.filled.has(10)).toBe(true);
    expect(cal.filled.has(15)).toBe(true);
    expect(cal.filled.has(1)).toBe(false);
  });

  it('ignores non-md files', () => {
    const now = new Date(2026, 4, 15);
    const tree: TreeNode[] = [
      { type: 'file', name: 'img.png', path: 'img.png', kind: 'image', updated: '2026-05-15T12:00:00Z', size: 500 },
    ];
    const cal = buildCalendar(now, now, tree);
    expect(cal.filled.size).toBe(0);
  });

  it('walks into folders', () => {
    const now = new Date(2026, 4, 20);
    const tree: TreeNode[] = [
      {
        type: 'folder',
        name: 'Journal',
        path: 'Journal',
        children: [mdFile('Journal/note.md', '2026-05-20T10:00:00Z')],
      },
    ];
    const cal = buildCalendar(now, now, tree);
    expect(cal.filled.has(20)).toBe(true);
  });

  it('renders the viewed month but only highlights today in the live month', () => {
    const today = new Date(2026, 4, 27);
    const browsed = buildCalendar(new Date(2026, 1, 1), today, []);
    expect(browsed.month).toBe(1);
    expect(browsed.daysInMonth).toBe(28);
    expect(browsed.today).toBe(0);
    expect(buildCalendar(new Date(2026, 4, 1), today, []).today).toBe(27);
  });
});

describe('entriesForDate', () => {
  it('returns entries for the specified date only', () => {
    const tree: TreeNode[] = [
      mdFile('a.md', '2026-05-10T08:00:00Z'),
      mdFile('b.md', '2026-05-10T14:00:00Z'),
      mdFile('c.md', '2026-05-11T09:00:00Z'),
    ];
    const date = new Date(2026, 4, 10);
    const entries = entriesForDate(date, tree, {}, null);
    expect(entries).toHaveLength(2);
    expect(entries[0].path).toBe('b.md');
    expect(entries[1].path).toBe('a.md');
  });

  it('marks the current file', () => {
    const tree: TreeNode[] = [mdFile('a.md', '2026-05-10T08:00:00Z')];
    const entries = entriesForDate(new Date(2026, 4, 10), tree, {}, 'a.md');
    expect(entries[0].current).toBe(true);
  });

  it('uses excerpt title when available', () => {
    const tree: TreeNode[] = [mdFile('a.md', '2026-05-10T08:00:00Z')];
    const excerpts = { 'a.md': { title: 'Custom Title', excerpt: '' } };
    const entries = entriesForDate(new Date(2026, 4, 10), tree, excerpts, null);
    expect(entries[0].title).toBe('Custom Title');
  });

  it('returns empty for a day with no entries', () => {
    const tree: TreeNode[] = [mdFile('a.md', '2026-05-10T08:00:00Z')];
    const entries = entriesForDate(new Date(2026, 4, 11), tree, {}, null);
    expect(entries).toHaveLength(0);
  });
});

describe('shortDayLabel', () => {
  it('formats month and day', () => {
    expect(shortDayLabel(4, 27)).toBe('27 May');
    expect(shortDayLabel(0, 1)).toBe('1 January');
  });
});
