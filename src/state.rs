use std::path::PathBuf;
use std::sync::{Arc, RwLock, RwLockReadGuard, RwLockWriteGuard};

use dashmap::DashMap;
use uuid::Uuid;

use crate::agent::AgentConfig;
use crate::error::{AppError, AppResult};
use crate::space::rate_limit::RateLimiter;
use crate::space::session::SessionStore;
use crate::space::users::UsersRegistry;
use crate::space::Space;

/// Runtime configuration sourced from the environment, shared by every handler.
#[derive(Clone, Debug)]
pub struct AppConfig {
    /// Operator-level gate for marking the session cookie `Secure`. Defaults to
    /// `true`; opt out with `SPACEIO_INSECURE_COOKIES=1` for plain-HTTP dev. The
    /// cookie is only actually `Secure` when this is set *and* the request
    /// arrived over HTTPS — browsers drop a `Secure` cookie over plain HTTP.
    pub cookie_secure: bool,
}

impl AppConfig {
    pub fn from_env() -> Self {
        let insecure = std::env::var("SPACEIO_INSECURE_COOKIES")
            .map(|v| v == "1" || v.eq_ignore_ascii_case("true"))
            .unwrap_or(false);
        Self {
            cookie_secure: !insecure,
        }
    }
}

/// Multi-tenant application state: a single `data/` root holds one
/// `.users.toml` mapping plus one `<uuid>/` subdirectory per registered user.
///
/// `spaces` caches opened `Space` handles by UUID to avoid re-parsing
/// `.space.toml` on every request. It is populated lazily and never invalidated
/// within a process; deploys restart the process.
#[derive(Clone)]
pub struct AppState {
    pub root: PathBuf,
    users: Arc<RwLock<UsersRegistry>>,
    spaces: Arc<DashMap<Uuid, Space>>,
    pub sessions: SessionStore,
    pub unlock_limiter: RateLimiter,
    pub config: AppConfig,
    /// Holds provider API keys, so it lives behind an `Arc` and never derives
    /// `Debug`.
    pub agent: Arc<AgentConfig>,
}

impl AppState {
    pub fn new(
        root: PathBuf,
        sessions: SessionStore,
        unlock_limiter: RateLimiter,
        config: AppConfig,
    ) -> AppResult<Self> {
        let users = UsersRegistry::load(&root)?;
        Ok(Self {
            root,
            users: Arc::new(RwLock::new(users)),
            spaces: Arc::new(DashMap::new()),
            sessions,
            unlock_limiter,
            config,
            agent: Arc::new(AgentConfig::from_env()),
        })
    }

    fn read_users(&self) -> RwLockReadGuard<'_, UsersRegistry> {
        self.users.read().expect("users rwlock poisoned")
    }

    fn write_users(&self) -> RwLockWriteGuard<'_, UsersRegistry> {
        self.users.write().expect("users rwlock poisoned")
    }

    pub fn any_users(&self) -> bool {
        !self.read_users().is_empty()
    }

    pub fn users_snapshot(&self) -> UsersRegistry {
        self.read_users().clone()
    }

    /// Register a new user. The caller must run `init_space` for the returned
    /// UUID separately — it is CPU-heavy and must not run under the registry
    /// lock.
    pub fn register_user(&self, email: &str) -> AppResult<crate::space::users::UserEntry> {
        self.write_users().add(&self.root, email)
    }

    /// Roll back a registration whose space failed to initialise. Best effort:
    /// a failed save leaves `.users.toml` for the operator to repair by hand.
    pub fn unregister_user(&self, uuid: &Uuid) {
        let mut guard = self.write_users();
        guard.remove_by_uuid(uuid);
        if let Err(e) = guard.save(&self.root) {
            tracing::warn!(error = %e, "failed to persist user-registry rollback");
        }
    }

    pub fn find_user_by_email(&self, email: &str) -> Option<crate::space::users::UserEntry> {
        self.read_users().find_by_email(email).cloned()
    }

    /// Open (or fetch from cache) the `Space` for a user UUID. Returns
    /// `NotFound` if the directory or `.space.toml` is missing.
    pub fn space_for(&self, uuid: &Uuid) -> AppResult<Space> {
        if let Some(s) = self.spaces.get(uuid) {
            return Ok(s.clone());
        }
        let dir = UsersRegistry::space_dir_for(&self.root, uuid);
        if !dir.is_dir() {
            return Err(AppError::NotFound);
        }
        let s = Space::open(dir)?;
        self.spaces.insert(*uuid, s.clone());
        Ok(s)
    }

    /// Install a freshly-initialised Space into the cache so the next request
    /// skips re-parsing the config.
    pub fn cache_space(&self, uuid: Uuid, space: Space) {
        self.spaces.insert(uuid, space);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::SpaceConfig;
    use crate::crypto::kdf;
    use rand::RngCore;
    use tempfile::TempDir;

    fn cheap_state() -> (TempDir, AppState) {
        let dir = TempDir::new().expect("tempdir");
        let state = AppState::new(
            dir.path().to_path_buf(),
            SessionStore::new(),
            RateLimiter::new(),
            AppConfig {
                cookie_secure: true,
            },
        )
        .expect("AppState::new");
        (dir, state)
    }

    /// Materialise a cheap-KDF space on disk at `<root>/<uuid>` so `space_for`
    /// can open it the way a real registration would.
    fn seed_space(root: &std::path::Path, uuid: &Uuid) {
        let user_dir = root.join(uuid.to_string());
        let space_root = SpaceConfig::space_root(&user_dir);
        std::fs::create_dir_all(&space_root).expect("mkdir space root");
        let mut salt = [0u8; 16];
        rand::thread_rng().fill_bytes(&mut salt);
        let verifier = kdf::derive_verifier("passphrase-9", &salt, 4, 8, 1).expect("kdf");
        let cfg = SpaceConfig {
            owner: "tester@home.lan".into(),
            salt_verify_hex: hex::encode(salt),
            verifier_hash_hex: hex::encode(verifier),
            kdf_log_n: 4,
            kdf_r: 8,
            kdf_p: 1,
            passkey: None,
        };
        cfg.save(&user_dir).expect("save config");
        git2::Repository::init(&space_root).expect("git init");
    }

    #[test]
    fn from_env_defaults_secure_and_honours_insecure_flag() {
        std::env::remove_var("SPACEIO_INSECURE_COOKIES");
        assert!(AppConfig::from_env().cookie_secure, "secure by default");
        std::env::set_var("SPACEIO_INSECURE_COOKIES", "1");
        assert!(!AppConfig::from_env().cookie_secure, "1 opts out");
        std::env::set_var("SPACEIO_INSECURE_COOKIES", "TRUE");
        assert!(!AppConfig::from_env().cookie_secure, "TRUE opts out");
        std::env::set_var("SPACEIO_INSECURE_COOKIES", "0");
        assert!(AppConfig::from_env().cookie_secure, "0 stays secure");
        std::env::remove_var("SPACEIO_INSECURE_COOKIES");
    }

    #[test]
    fn register_find_and_snapshot_users() {
        let (_d, state) = cheap_state();
        assert!(!state.any_users());
        let entry = state.register_user("ada@example.lan").expect("register");
        assert!(state.any_users());
        let found = state
            .find_user_by_email("ada@example.lan")
            .expect("user is findable by email");
        assert_eq!(found.uuid, entry.uuid);
        assert!(state.find_user_by_email("nobody@nowhere.lan").is_none());
        assert_eq!(state.users_snapshot().users.len(), 1);
    }

    #[test]
    fn space_for_missing_dir_is_not_found() {
        let (_d, state) = cheap_state();
        let entry = state.register_user("ada@example.lan").expect("register");
        assert!(matches!(
            state.space_for(&entry.uuid),
            Err(AppError::NotFound)
        ));
    }

    #[test]
    fn space_for_opens_from_disk_then_serves_from_cache() {
        let (dir, state) = cheap_state();
        let entry = state.register_user("ada@example.lan").expect("register");
        seed_space(dir.path(), &entry.uuid);
        let opened = state.space_for(&entry.uuid).expect("opens from disk");
        let cached = state.space_for(&entry.uuid).expect("served from cache");
        assert_eq!(opened.config().owner, cached.config().owner);
    }

    #[test]
    fn cache_space_short_circuits_disk_lookup() {
        let (_d, state) = cheap_state();
        let id = Uuid::new_v4();
        // Prime the cache with a space living in its own tempdir; `space_for`
        // must return it without consulting <root>/<uuid>, which never exists.
        let (_sd, space, _pass) = crate::space::test_helpers::make_space("p");
        state.cache_space(id, space);
        assert!(state.space_for(&id).is_ok());
    }

    #[test]
    fn unregister_user_rolls_back_the_registry() {
        let (_d, state) = cheap_state();
        let entry = state.register_user("ada@example.lan").expect("register");
        assert!(state.any_users());
        state.unregister_user(&entry.uuid);
        assert!(!state.any_users());
        assert!(state.find_user_by_email("ada@example.lan").is_none());
    }
}
