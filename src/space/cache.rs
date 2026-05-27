use std::collections::HashMap;
use std::sync::{Arc, Mutex};
use std::time::SystemTime;

/// Per-file decrypted-content cache used by `search` and `excerpt`.
///
/// We key by encrypted-file path + mtime: when the file changes on disk the
/// mtime moves and the entry is invalidated automatically. Cache hits avoid
/// the age decrypt (and its scrypt-derived passphrase verification) per
/// search request, which dominated wall-clock time on every keystroke.
#[derive(Clone, Default)]
pub struct DecryptedCache {
    inner: Arc<Mutex<HashMap<String, Entry>>>,
}

struct Entry {
    mtime: SystemTime,
    text: Arc<str>,
}

impl DecryptedCache {
    pub fn new() -> Self {
        Self::default()
    }

    /// Look up cached plaintext for `key`. Returns `None` if absent or if
    /// the on-disk mtime no longer matches the cached one.
    pub fn get(&self, key: &str, mtime: SystemTime) -> Option<Arc<str>> {
        let guard = self.inner.lock().ok()?;
        let entry = guard.get(key)?;
        if entry.mtime == mtime {
            Some(entry.text.clone())
        } else {
            None
        }
    }

    pub fn put(&self, key: String, mtime: SystemTime, text: Arc<str>) {
        if let Ok(mut guard) = self.inner.lock() {
            guard.insert(key, Entry { mtime, text });
        }
    }

    /// Drop the entry for `key` — used when a file is moved, deleted, or
    /// rewritten so a subsequent search doesn't return stale plaintext.
    pub fn invalidate(&self, key: &str) {
        if let Ok(mut guard) = self.inner.lock() {
            guard.remove(key);
        }
    }

    pub fn clear(&self) {
        if let Ok(mut guard) = self.inner.lock() {
            guard.clear();
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::Duration;

    #[test]
    fn get_returns_none_when_empty() {
        let c = DecryptedCache::new();
        assert!(c.get("k", SystemTime::now()).is_none());
    }

    #[test]
    fn put_then_get_round_trips() {
        let c = DecryptedCache::new();
        let t = SystemTime::now();
        c.put("k".into(), t, Arc::from("hello"));
        assert_eq!(c.get("k", t).as_deref(), Some("hello"));
    }

    #[test]
    fn mtime_change_invalidates() {
        let c = DecryptedCache::new();
        let t = SystemTime::now();
        c.put("k".into(), t, Arc::from("hello"));
        let t2 = t + Duration::from_secs(1);
        assert!(c.get("k", t2).is_none());
    }

    #[test]
    fn invalidate_removes_entry() {
        let c = DecryptedCache::new();
        let t = SystemTime::now();
        c.put("k".into(), t, Arc::from("hello"));
        c.invalidate("k");
        assert!(c.get("k", t).is_none());
    }
}
