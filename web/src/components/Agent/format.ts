//! Pure helpers for rendering the assistant transcript. Kept out of the
//! component so they can be unit-tested without React.

/** Coerce an unknown tool-argument value to a display string. */
export function str(v: unknown): string {
  if (typeof v === 'string') return v;
  if (v == null) return '';
  return String(v);
}

/** Parse a tool call's JSON `arguments` string, tolerating malformed input. */
export function safeParse(raw: string): Record<string, unknown> {
  try {
    const v = JSON.parse(raw);
    return v && typeof v === 'object' ? (v as Record<string, unknown>) : {};
  } catch {
    return {};
  }
}

/** Friendly one-liner describing a tool call, shown in the transcript. */
export function toolLabel(name: string, rawArgs: string): string {
  const a = safeParse(rawArgs);
  switch (name) {
    case 'list_files':
      return 'Listed the vault';
    case 'read_file':
      return `Read ${str(a.path)}`;
    case 'search_notes':
      return `Searched “${str(a.query)}”`;
    case 'web_search':
      return `Searched the web for “${str(a.query)}”`;
    case 'write_file':
      return `Wrote ${str(a.path)}`;
    case 'move_path':
      return `Moved ${str(a.from)} → ${str(a.to)}`;
    case 'delete_path':
      return `Deleted ${str(a.path)}`;
    case 'create_folder':
      return `Created folder ${str(a.path)}`;
    case 'set_tags':
      return `Tagged ${str(a.path)}`;
    default:
      return name;
  }
}
