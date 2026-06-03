use std::path::PathBuf;
use std::sync::{Arc, RwLock};

use dashmap::DashMap;
use uuid::Uuid;

use crate::error::{AppError, AppResult};
use crate::space::rate_limit::RateLimiter;
use crate::space::session::SessionStore;
use crate::space::users::UsersRegistry;
use crate::space::Space;

/// Runtime configuration sourced from CLI flags + env. Shared by every
/// request handler so we don't sprinkle `std::env::var` lookups around.
#[derive(Clone, Debug)]
pub struct AppConfig {
    /// Whether the session cookie *may* be marked `Secure`. Defaults to
    /// `true`; opt out with `HEARTH_INSECURE_COOKIES=1` for local plain-HTTP
    /// development. This is only the operator-level gate — the cookie is
    /// additionally marked `Secure` *per request* only when the request
    /// actually arrived over HTTPS (`X-Forwarded-Proto: https`). A `Secure`
    /// cookie is dropped by the browser over plain HTTP, which would log the
    /// user out on refresh, so both conditions must hold.
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

/// Application state. Multi-tenant: a single `data/` root holds one
/// `.users.toml` mapping plus one `<uuid>/` subdirectory per registered user.
///
/// `spaces` caches opened `Space` handles by user UUID to avoid re-parsing
/// `.space.toml` on every request. Cache is populated lazily and never
/// invalidated within a process — passkey writes flow through the cached
/// handle, and we restart the process on `hearth serve` deploys.
#[derive(Clone)]
pub struct AppState {
    /// `./data` — the directory that holds `.users.toml` and the per-user
    /// subdirectories.
    pub root: PathBuf,
    users: Arc<RwLock<UsersRegistry>>,
    spaces: Arc<DashMap<Uuid, Space>>,
    pub sessions: SessionStore,
    pub unlock_limiter: RateLimiter,
    pub config: AppConfig,
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
        })
    }

    pub fn any_users(&self) -> bool {
        !self.users.read().expect("users rwlock poisoned").is_empty()
    }

    /// Cheap snapshot of the registry. Useful when callers want to scan
    /// entries without holding the lock.
    pub fn users_snapshot(&self) -> UsersRegistry {
        self.users.read().expect("users rwlock poisoned").clone()
    }

    /// Register a new user and create their per-UUID space directory marker.
    /// Returns the new user's UUID. The caller is responsible for running
    /// `init_space` inside the per-UUID directory (it's CPU-heavy and runs on
    /// a blocking thread; we don't hide that latency behind the registry lock).
    pub fn register_user(&self, email: &str) -> AppResult<crate::space::users::UserEntry> {
        let mut guard = self.users.write().expect("users rwlock poisoned");
        guard.add(&self.root, email)
    }

    /// Roll back a failed registration: remove the entry whose UUID we just
    /// minted but whose space we couldn't initialise. Best effort — if the
    /// save fails the operator can still hand-edit `.users.toml`, but the
    /// common case (init_space errors after the registry write) leaves a
    /// clean state for the next attempt.
    pub fn unregister_user(&self, uuid: &Uuid) {
        let mut guard = self.users.write().expect("users rwlock poisoned");
        guard.remove_by_uuid(uuid);
        if let Err(e) = guard.save(&self.root) {
            tracing::warn!(error = %e, "failed to persist user-registry rollback");
        }
    }

    /// Find a user by email (case-insensitive).
    pub fn find_user_by_email(&self, email: &str) -> Option<crate::space::users::UserEntry> {
        self.users
            .read()
            .expect("users rwlock poisoned")
            .find_by_email(email)
            .cloned()
    }

    /// Open (or fetch from cache) the `Space` for a given user UUID.
    ///
    /// Returns `NotFound` if the directory or `.space.toml` is missing — the
    /// registry pointed at a user that no longer has a backing space, which
    /// shouldn't happen but is worth surfacing rather than panicking.
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

    /// Install a freshly-initialised Space into the cache. Used right after
    /// registration so the very next request doesn't re-parse the config.
    pub fn cache_space(&self, uuid: Uuid, space: Space) {
        self.spaces.insert(uuid, space);
    }
}
