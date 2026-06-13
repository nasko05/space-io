pub mod cache;
pub mod create;
pub mod delete;
pub mod download;
pub mod excerpt;
pub mod git;
pub mod history;
pub mod init;
pub mod meta;
pub mod mkdir;
pub mod paths;
pub mod rate_limit;
pub mod read;
pub mod rename;
pub mod rollback;
pub mod search;
pub mod session;
pub mod tree;
pub mod upload;
pub mod users;
pub mod write;

#[cfg(test)]
pub(crate) mod test_helpers;

use std::path::PathBuf;
use std::sync::{Arc, Mutex, RwLock};
use std::time::SystemTime;

use time::OffsetDateTime;

use crate::config::{PasskeyConfig, SpaceConfig};
use crate::error::{AppError, AppResult};
use crate::space::cache::DecryptedCache;
use crate::space::meta::MetaCache;

/// Format a `SystemTime` as an RFC 3339 string, or `None` if conversion fails.
pub fn systemtime_iso8601(t: SystemTime) -> Option<String> {
    let dt: OffsetDateTime = t.into();
    dt.format(&time::format_description::well_known::Rfc3339)
        .ok()
}

#[derive(Clone)]
pub struct Space {
    inner: Arc<SpaceInner>,
}

struct SpaceInner {
    space_dir: PathBuf,
    config: RwLock<SpaceConfig>,
    /// Opened once at startup (opening scans pack indices and refs). `!Sync`,
    /// hence the `Mutex`.
    repo: Mutex<git2::Repository>,
    /// Decrypted plaintext cache for search and excerpt builders, tied to the
    /// unlocked vault and dropped with `Space`.
    decrypted: DecryptedCache,
    /// In-memory mirror of the age-encrypted meta index, so tag edits, search,
    /// rename, and delete don't pay the scrypt KDF cost each time.
    meta_cache: MetaCache,
}

impl Space {
    pub fn open(space_dir: PathBuf) -> AppResult<Self> {
        let config = SpaceConfig::load(&space_dir)?;
        let root = SpaceConfig::space_root(&space_dir);
        if !root.is_dir() {
            return Err(AppError::Internal(format!(
                "space root missing: {}",
                root.display()
            )));
        }
        let repo = git::open(&root)?;
        Ok(Self {
            inner: Arc::new(SpaceInner {
                space_dir,
                config: RwLock::new(config),
                repo: Mutex::new(repo),
                decrypted: DecryptedCache::new(),
                meta_cache: MetaCache::new(),
            }),
        })
    }

    /// Shared cache for decrypted markdown bodies. Read by `search` and
    /// `excerpt`; invalidated by `write` / `delete` / `rename`.
    pub fn cache(&self) -> &DecryptedCache {
        &self.inner.decrypted
    }

    /// Cached parsed meta index. Read/written by `space::meta`.
    pub fn meta_cache(&self) -> &MetaCache {
        &self.inner.meta_cache
    }

    pub fn root(&self) -> PathBuf {
        SpaceConfig::space_root(&self.inner.space_dir)
    }

    /// Run a closure against the cached git repository under the internal mutex.
    /// Keep it short: this lock serialises writes across the whole vault.
    pub fn with_repo<R, F>(&self, f: F) -> AppResult<R>
    where
        F: FnOnce(&git2::Repository) -> AppResult<R>,
    {
        let guard = self
            .inner
            .repo
            .lock()
            .map_err(|_| AppError::Internal("git repo mutex poisoned".into()))?;
        f(&guard)
    }

    /// Cloned snapshot of the current config so callers read fields without
    /// holding the lock.
    pub fn config(&self) -> SpaceConfig {
        self.inner
            .config
            .read()
            .expect("config rwlock poisoned")
            .clone()
    }

    pub fn set_passkey(&self, passkey: Option<PasskeyConfig>) -> AppResult<()> {
        let mut guard = self
            .inner
            .config
            .write()
            .map_err(|_| AppError::Internal("config rwlock poisoned".into()))?;
        guard.passkey = passkey;
        guard.save(&self.inner.space_dir)?;
        Ok(())
    }
}
