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
    /// `true`; opt out with `HEARTH_INSECURE_COOKIES=1` for plain-HTTP dev. The
    /// cookie is only actually `Secure` when this is set *and* the request
    /// arrived over HTTPS — browsers drop a `Secure` cookie over plain HTTP.
    pub cookie_secure: bool,
}

impl AppConfig {
    pub fn from_env() -> Self {
        let insecure = std::env::var("HEARTH_INSECURE_COOKIES")
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
