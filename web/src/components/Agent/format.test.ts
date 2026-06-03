import { describe, expect, it } from 'vitest';
import { safeParse, str, toolLabel } from './format';

describe('str', () => {
  it('passes strings through and stringifies the rest', () => {
    expect(str('a.md')).toBe('a.md');
    expect(str(null)).toBe('');
    expect(str(undefined)).toBe('');
    expect(str(42)).toBe('42');
  });
});

describe('safeParse', () => {
  it('parses a JSON object', () => {
    expect(safeParse('{"path":"a.md"}')).toEqual({ path: 'a.md' });
  });

  it('returns an empty object for malformed or non-object JSON', () => {
    expect(safeParse('{not json')).toEqual({});
    expect(safeParse('"a string"')).toEqual({});
    expect(safeParse('')).toEqual({});
  });
});

describe('toolLabel', () => {
  it('describes read-only calls', () => {
    expect(toolLabel('list_files', '{}')).toBe('Listed the vault');
    expect(toolLabel('read_file', '{"path":"Journal/2026/n.md"}')).toBe('Read Journal/2026/n.md');
    expect(toolLabel('search_notes', '{"query":"fox"}')).toContain('fox');
    expect(toolLabel('web_search', '{"query":"weather"}')).toContain('web');
  });

  it('describes mutating calls', () => {
    expect(toolLabel('write_file', '{"path":"a.md"}')).toBe('Wrote a.md');
    expect(toolLabel('move_path', '{"from":"a.md","to":"b.md"}')).toBe('Moved a.md → b.md');
    expect(toolLabel('delete_path', '{"path":"a.md"}')).toBe('Deleted a.md');
    expect(toolLabel('create_folder', '{"path":"New"}')).toBe('Created folder New');
    expect(toolLabel('set_tags', '{"path":"a.md","tags":["x"]}')).toBe('Tagged a.md');
  });

  it('falls back to the raw tool name and tolerates bad args', () => {
    expect(toolLabel('mystery_tool', '{}')).toBe('mystery_tool');
    expect(toolLabel('read_file', 'not json')).toBe('Read ');
  });
});
