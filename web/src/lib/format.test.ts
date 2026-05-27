import { describe, it, expect } from 'vitest';
import { formatSize, shortId } from './format';

describe('formatSize', () => {
  it('formats bytes', () => {
    expect(formatSize(0)).toBe('0 B');
    expect(formatSize(512)).toBe('512 B');
  });

  it('formats kilobytes', () => {
    expect(formatSize(1024)).toBe('1.0 KB');
    expect(formatSize(1536)).toBe('1.5 KB');
  });

  it('formats megabytes', () => {
    expect(formatSize(1024 * 1024)).toBe('1.0 MB');
    expect(formatSize(5 * 1024 * 1024)).toBe('5.0 MB');
  });
});

describe('shortId', () => {
  it('generates unique ids', () => {
    const a = shortId();
    const b = shortId();
    expect(a).not.toBe(b);
  });

  it('uses the given prefix', () => {
    const id = shortId('test');
    expect(id.startsWith('test-')).toBe(true);
  });
});
