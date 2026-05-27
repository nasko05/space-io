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
