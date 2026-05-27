use std::sync::Arc;

use age::secrecy::SecretString;
use dashmap::DashMap;
use uuid::Uuid;

/// In-memory session store: session_id → live passphrase.
/// Lifetime ends when the process exits or when the user locks.
#[derive(Clone, Default)]
pub struct SessionStore {
    inner: Arc<DashMap<Uuid, SecretString>>,
}

impl SessionStore {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn create(&self, passphrase: SecretString) -> Uuid {
        let id = Uuid::new_v4();
        self.inner.insert(id, passphrase);
        id
    }

    pub fn get(&self, id: &Uuid) -> Option<SecretString> {
        self.inner.get(id).map(|r| r.clone())
    }

    pub fn drop(&self, id: &Uuid) {
        self.inner.remove(id);
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
}
