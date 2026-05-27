use std::sync::Arc;

use age::secrecy::SecretString;
use dashmap::DashMap;
use uuid::Uuid;

/// One live unlock. Stores the passphrase (needed to decrypt files) and the
/// UUID of the user's space (needed to route requests to the right
/// directory). Lifetime ends when the process exits or when the user locks.
#[derive(Clone)]
pub struct Session {
    pub passphrase: SecretString,
    pub user_uuid: Uuid,
}

/// In-memory session store. `session_id → Session`.
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
        self.inner.insert(
            id,
            Session {
                passphrase,
                user_uuid,
            },
        );
        id
    }

    pub fn get(&self, id: &Uuid) -> Option<Session> {
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
}
