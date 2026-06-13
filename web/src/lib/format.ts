/** Format a byte count as "N B" / "N.N KB" / "N.N MB". */
export function formatSize(bytes: number): string {
  if (bytes < 1024) return `${bytes} B`;
  if (bytes < 1024 * 1024) return `${(bytes / 1024).toFixed(1)} KB`;
  return `${(bytes / 1024 / 1024).toFixed(1)} MB`;
}

/** Short id, unique within a component's lifetime. Uses `crypto.randomUUID`
 * when available and falls back to a counter under jsdom / older browsers. */
let monotonic = 0;
export function shortId(prefix = 'id'): string {
  if (typeof crypto !== 'undefined' && typeof crypto.randomUUID === 'function') {
    return `${prefix}-${crypto.randomUUID()}`;
  }
  monotonic += 1;
  return `${prefix}-${Date.now().toString(36)}-${monotonic.toString(36)}`;
}
