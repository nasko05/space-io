use std::collections::HashMap;
use std::sync::{Arc, Mutex};
use std::time::SystemTime;

/// Per-file decrypted-content cache used by `search` and `excerpt`, keyed by
/// encrypted-file path + mtime so a changed file invalidates automatically.
/// Hits skip the age decrypt (and its slow scrypt verification) that otherwise
/// dominated every keystroke. Entries also memoize an ASCII-lowercased mirror of
/// the body so case-insensitive search lowercases each file only once.
#[derive(Clone, Default)]
pub struct DecryptedCache {
    inner: Arc<Mutex<HashMap<String, Entry>>>,
}

struct Entry {
    mtime: SystemTime,
    text: Arc<str>,
    lowered: Option<Arc<str>>,
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

    /// Look up cached plaintext plus its ASCII-lowercased mirror, computed on
    /// first request and reused until the underlying file changes.
    pub fn get_with_lowered(&self, key: &str, mtime: SystemTime) -> Option<(Arc<str>, Arc<str>)> {
        let mut guard = self.inner.lock().ok()?;
        let entry = guard.get_mut(key)?;
        if entry.mtime != mtime {
            return None;
        }
        let text = entry.text.clone();
        let lowered = match &entry.lowered {
            Some(lowered) => lowered.clone(),
            None => {
                let lowered: Arc<str> = Arc::from(text.to_ascii_lowercase());
                entry.lowered = Some(lowered.clone());
                lowered
            }
        };
        Some((text, lowered))
    }

    pub fn put(&self, key: String, mtime: SystemTime, text: Arc<str>) {
        if let Ok(mut guard) = self.inner.lock() {
            guard.insert(
                key,
                Entry {
                    mtime,
                    text,
                    lowered: None,
                },
            );
        }
    }

    /// Drop the entry for `key` when a file is moved, deleted, or rewritten, so a
    /// later search can't return stale plaintext.
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

    #[test]
    fn get_with_lowered_returns_lowered_text() {
        let c = DecryptedCache::new();
        let t = SystemTime::now();
        c.put("k".into(), t, Arc::from("Hello WORLD"));
        let (text, lower) = c.get_with_lowered("k", t).unwrap();
        assert_eq!(&*text, "Hello WORLD");
        assert_eq!(&*lower, "hello world");
    }

    #[test]
    fn get_with_lowered_caches_subsequent_calls() {
        let c = DecryptedCache::new();
        let t = SystemTime::now();
        c.put("k".into(), t, Arc::from("Hi"));
        let (_, first) = c.get_with_lowered("k", t).unwrap();
        let (_, second) = c.get_with_lowered("k", t).unwrap();
        assert!(Arc::ptr_eq(&first, &second), "lowered Arc should be reused");
    }

    #[test]
    fn get_with_lowered_misses_when_mtime_changes() {
        let c = DecryptedCache::new();
        let t = SystemTime::now();
        c.put("k".into(), t, Arc::from("Hi"));
        let t2 = t + Duration::from_secs(1);
        assert!(c.get_with_lowered("k", t2).is_none());
    }
}
