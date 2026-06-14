/** Coerce an unknown tool-argument value to a display string. */
export function str(value: unknown): string {
  if (typeof value === 'string') { return value; }
  if (value == null) { return ''; }
  return String(value);
}

/** Parse a tool call's JSON `arguments` string, tolerating malformed input. */
export function safeParse(raw: string): Record<string, unknown> {
  try {
    const parsed = JSON.parse(raw);
    return parsed && typeof parsed === 'object' ? (parsed as Record<string, unknown>) : {};
  } catch {
    return {};
  }
}

/** Friendly one-liner describing a tool call, shown in the transcript. */
export function toolLabel(name: string, rawArgs: string): string {
  const args = safeParse(rawArgs);
  switch (name) {
    case 'list_files':
      return 'Listed the vault';
    case 'read_file':
      return `Read ${str(args.path)}`;
    case 'search_notes':
      return `Searched “${str(args.query)}”`;
    case 'web_search':
      return `Searched the web for “${str(args.query)}”`;
    case 'write_file':
      return `Wrote ${str(args.path)}`;
    case 'move_path':
      return `Moved ${str(args.from)} → ${str(args.to)}`;
    case 'delete_path':
      return `Deleted ${str(args.path)}`;
    case 'create_folder':
      return `Created folder ${str(args.path)}`;
    case 'set_tags':
      return `Tagged ${str(args.path)}`;
    default:
      return name;
  }
}
