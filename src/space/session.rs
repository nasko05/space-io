use std::sync::Arc;
use std::time::{Duration, Instant};

use age::secrecy::SecretString;
use dashmap::DashMap;
use uuid::Uuid;

/// Sliding idle window: a session survives this long after the last touch.
/// Seven days keeps people logged in across a normal week of intermittent
/// use (close the laptop Friday, pick up Monday) without a fresh sign-in,
/// while still reaping tabs that go genuinely cold.
pub const SESSION_IDLE_TTL: Duration = Duration::from_secs(7 * 24 * 60 * 60);

/// Hard ceiling on a session's total lifetime, regardless of activity. Even
/// a continuously-used session is forced back through the passphrase after
/// 30 days so a leaked cookie can't stay a standing vault key forever.
pub const SESSION_ABSOLUTE_TTL: Duration = Duration::from_secs(30 * 24 * 60 * 60);

/// One live unlock. Bundles everything a request needs to read the user's
/// space: the passphrase (to decrypt) and the user's UUID (to route to the
/// right per-user directory). `last_seen` slides forward on every get() so
/// active sessions stay alive within the idle window; `created_at` is fixed
/// at unlock time and enforces the absolute lifetime cap.
#[derive(Clone)]
pub struct Session {
    pub passphrase: SecretString,
    pub user_uuid: Uuid,
    pub last_seen: Instant,
    pub created_at: Instant,
}

/// In-memory session store: session_id → Session.
/// Lifetime ends when the process exits, when the user locks, when the
/// session has been idle past `SESSION_IDLE_TTL`, or once it crosses the
/// `SESSION_ABSOLUTE_TTL` hard cap.
#[derive(Clone, Default)]
pub struct SessionStore {
    inner: Arc<DashMap<Uuid, Session>>,
}

impl SessionStore {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn create(&self, passphrase: SecretString, user_uuid: Uuid) -> Uuid {
        let id = Uuid::new_v4();
        let now = Instant::now();
        self.inner.insert(
            id,
            Session {
                passphrase,
                user_uuid,
                last_seen: now,
                created_at: now,
            },
        );
        id
    }

    /// Resolve a session, sliding the idle window. Returns `None` if the
    /// session is unknown or has expired — either by going idle past
    /// `SESSION_IDLE_TTL` or by crossing the `SESSION_ABSOLUTE_TTL` hard cap
    /// (in which case the expired entry is dropped on the spot).
    pub fn get(&self, id: &Uuid) -> Option<Session> {
        let now = Instant::now();
        let mut entry = self.inner.get_mut(id)?;
        let idle_expired = now.duration_since(entry.last_seen) > SESSION_IDLE_TTL;
        let absolute_expired = now.duration_since(entry.created_at) > SESSION_ABSOLUTE_TTL;
        if idle_expired || absolute_expired {
            drop(entry);
            self.inner.remove(id);
            return None;
        }
        entry.last_seen = now;
        Some(entry.clone())
    }

    pub fn drop(&self, id: &Uuid) {
        self.inner.remove(id);
    }

    /// Walk the table and evict everything past either TTL. Cheap enough to
    /// call from a periodic task; `O(n)` over a tiny `n`. A session is kept
    /// only if it is within BOTH the sliding idle window and the absolute cap.
    pub fn sweep_expired(&self) {
        let now = Instant::now();
        self.inner.retain(|_, s| {
            now.duration_since(s.last_seen) <= SESSION_IDLE_TTL
                && now.duration_since(s.created_at) <= SESSION_ABSOLUTE_TTL
        });
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
        let a = store.create(secret("one"), Uuid::new_v4());
        let b = store.create(secret("two"), Uuid::new_v4());
        assert_ne!(a, b);
    }

    #[test]
    fn get_returns_stored_session() {
        let store = SessionStore::new();
        let user = Uuid::new_v4();
        let id = store.create(secret("mine"), user);
        let s = store.get(&id).expect("present");
        assert_eq!(s.passphrase.expose_secret(), "mine");
        assert_eq!(s.user_uuid, user);
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
        let id = store.create(secret("one"), Uuid::new_v4());
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
        let id = a.create(secret("shared"), Uuid::new_v4());
        assert!(b.get(&id).is_some());
    }

    #[test]
    fn idle_expired_session_is_evicted_on_get() {
        let store = SessionStore::new();
        let id = store.create(secret("stale"), Uuid::new_v4());
        // Backdate last_seen so the next get() trips the idle window.
        store.inner.get_mut(&id).expect("present").last_seen =
            Instant::now() - SESSION_IDLE_TTL - Duration::from_secs(1);
        assert!(store.get(&id).is_none());
        // And the entry is gone, not just hidden.
        assert!(store.inner.get(&id).is_none());
    }

    #[test]
    fn absolutely_capped_session_is_evicted_on_get_even_when_recently_seen() {
        let store = SessionStore::new();
        let id = store.create(secret("old"), Uuid::new_v4());
        {
            // Active right now (last_seen = now) but minted long ago: the
            // absolute cap must override the still-fresh idle window.
            let mut entry = store.inner.get_mut(&id).expect("present");
            entry.last_seen = Instant::now();
            entry.created_at = Instant::now() - SESSION_ABSOLUTE_TTL - Duration::from_secs(1);
        }
        assert!(store.get(&id).is_none());
        assert!(store.inner.get(&id).is_none());
    }

    #[test]
    fn sweep_evicts_idle_and_absolutely_capped_entries() {
        let store = SessionStore::new();
        let fresh = store.create(secret("fresh"), Uuid::new_v4());
        let idle = store.create(secret("idle"), Uuid::new_v4());
        let capped = store.create(secret("capped"), Uuid::new_v4());
        // Idle past the sliding window.
        store.inner.get_mut(&idle).expect("present").last_seen =
            Instant::now() - SESSION_IDLE_TTL - Duration::from_secs(1);
        // Recently seen, but past the absolute cap.
        {
            let mut entry = store.inner.get_mut(&capped).expect("present");
            entry.last_seen = Instant::now();
            entry.created_at = Instant::now() - SESSION_ABSOLUTE_TTL - Duration::from_secs(1);
        }
        store.sweep_expired();
        assert!(store.inner.get(&fresh).is_some());
        assert!(store.inner.get(&idle).is_none());
        assert!(store.inner.get(&capped).is_none());
    }
}
