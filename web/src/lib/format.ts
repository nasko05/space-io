/** Format a byte count as "N B" / "N.N KB" / "N.N MB". Mirrors the
 * pre-existing helper that was duplicated across UploadModal,
 * DownloadModal, Preview, and HearthCard. */
export function formatSize(bytes: number): string {
  if (bytes < 1024) return `${bytes} B`;
  if (bytes < 1024 * 1024) return `${(bytes / 1024).toFixed(1)} KB`;
  return `${(bytes / 1024 / 1024).toFixed(1)} MB`;
}

/** Crypto-strong-when-available short id. Falls back to a counter-style
 * string under jsdom / older browsers where `crypto.randomUUID` is
 * missing. The id only needs to be unique within a single component's
 * lifetime, so the fallback is fine. */
let monotonic = 0;
export function shortId(prefix = 'id'): string {
  if (typeof crypto !== 'undefined' && typeof crypto.randomUUID === 'function') {
    return `${prefix}-${crypto.randomUUID()}`;
  }
  monotonic += 1;
  return `${prefix}-${Date.now().toString(36)}-${monotonic.toString(36)}`;
}
