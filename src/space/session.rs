use std::sync::Arc;
use std::time::{Duration, Instant};

use age::secrecy::SecretString;
use dashmap::DashMap;
use uuid::Uuid;

/// How long a session stays valid after the last touch. Eight hours is a
/// reasonable workday; longer than that and an unlocked browser tab becomes
/// a standing vault key for whoever stumbles past the screen.
pub const SESSION_TTL: Duration = Duration::from_secs(8 * 60 * 60);

struct Entry {
    passphrase: SecretString,
    last_seen: Instant,
}

/// In-memory session store: session_id → (passphrase, last_seen).
/// Lifetime ends when the process exits, when the user locks, or when the
/// session has been idle past `SESSION_TTL`.
#[derive(Clone, Default)]
pub struct SessionStore {
    inner: Arc<DashMap<Uuid, Entry>>,
}

impl SessionStore {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn create(&self, passphrase: SecretString) -> Uuid {
        let id = Uuid::new_v4();
        self.inner.insert(
            id,
            Entry {
                passphrase,
                last_seen: Instant::now(),
            },
        );
        id
    }

    /// Resolve a session, sliding the TTL window. Returns `None` if the
    /// session is unknown or has expired (in which case the expired entry
    /// is dropped on the spot).
    pub fn get(&self, id: &Uuid) -> Option<SecretString> {
        let now = Instant::now();
        let mut entry = self.inner.get_mut(id)?;
        if now.duration_since(entry.last_seen) > SESSION_TTL {
            drop(entry);
            self.inner.remove(id);
            return None;
        }
        entry.last_seen = now;
        Some(entry.passphrase.clone())
    }

    pub fn drop(&self, id: &Uuid) {
        self.inner.remove(id);
    }

    /// Walk the table and evict everything past TTL. Cheap enough to call
    /// from a periodic task; `O(n)` over a tiny `n`.
    pub fn sweep_expired(&self) {
        let now = Instant::now();
        self.inner
            .retain(|_, e| now.duration_since(e.last_seen) <= SESSION_TTL);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use age::secrecy::ExposeSecret;

    fn secret(s: &str) -> SecretString {
        SecretString::from(s.to_string())
    }

    #[test]
    fn create_returns_unique_ids() {
        let store = SessionStore::new();
        let a = store.create(secret("one"));
        let b = store.create(secret("two"));
        assert_ne!(a, b);
    }

    #[test]
    fn get_returns_stored_passphrase() {
        let store = SessionStore::new();
        let id = store.create(secret("mine"));
        let s = store.get(&id).expect("present");
        assert_eq!(s.expose_secret(), "mine");
    }

    #[test]
    fn get_returns_none_for_unknown_id() {
        let store = SessionStore::new();
        let unknown = Uuid::new_v4();
        assert!(store.get(&unknown).is_none());
    }

    #[test]
    fn drop_removes_the_session() {
        let store = SessionStore::new();
        let id = store.create(secret("one"));
        store.drop(&id);
        assert!(store.get(&id).is_none());
    }

    #[test]
    fn drop_on_unknown_id_is_a_noop() {
        let store = SessionStore::new();
        store.drop(&Uuid::new_v4());
    }

    #[test]
    fn store_clones_share_state() {
        let a = SessionStore::new();
        let b = a.clone();
        let id = a.create(secret("shared"));
        assert!(b.get(&id).is_some());
    }

    #[test]
    fn expired_session_is_evicted_on_get() {
        let store = SessionStore::new();
        let id = store.create(secret("stale"));
        // Backdate the entry so the next get() trips the TTL.
        store
            .inner
            .get_mut(&id)
            .expect("present")
            .last_seen = Instant::now() - SESSION_TTL - Duration::from_secs(1);
        assert!(store.get(&id).is_none());
        // And the entry is gone, not just hidden.
        assert!(store.inner.get(&id).is_none());
    }

    #[test]
    fn sweep_evicts_only_expired_entries() {
        let store = SessionStore::new();
        let fresh = store.create(secret("fresh"));
        let stale = store.create(secret("stale"));
        store
            .inner
            .get_mut(&stale)
            .expect("present")
            .last_seen = Instant::now() - SESSION_TTL - Duration::from_secs(1);
        store.sweep_expired();
        assert!(store.inner.get(&fresh).is_some());
        assert!(store.inner.get(&stale).is_none());
    }
}
