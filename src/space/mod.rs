pub mod create;
pub mod download;
pub mod excerpt;
pub mod git;
pub mod history;
pub mod init;
pub mod paths;
pub mod read;
pub mod search;
pub mod session;
pub mod tree;
pub mod upload;
pub mod write;

#[cfg(test)]
pub(crate) mod test_helpers;

use std::path::PathBuf;
use std::sync::{Arc, RwLock};

use crate::config::{PasskeyConfig, SpaceConfig};
use crate::error::{AppError, AppResult};

#[derive(Clone)]
pub struct Space {
    inner: Arc<SpaceInner>,
}

struct SpaceInner {
    space_dir: PathBuf,
    config: RwLock<SpaceConfig>,
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
        Ok(Self {
            inner: Arc::new(SpaceInner {
                space_dir,
                config: RwLock::new(config),
            }),
        })
    }

    pub fn root(&self) -> PathBuf {
        SpaceConfig::space_root(&self.inner.space_dir)
    }

    /// Cheap snapshot of the current on-disk config — clone so callers can
    /// read fields without holding the lock.
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
