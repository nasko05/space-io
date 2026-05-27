pub mod init;
pub mod read;
pub mod session;
pub mod tree;

use std::path::PathBuf;
use std::sync::Arc;

use crate::config::SpaceConfig;
use crate::error::{AppError, AppResult};

#[derive(Clone)]
pub struct Space {
    inner: Arc<SpaceInner>,
}

struct SpaceInner {
    space_dir: PathBuf,
    config: SpaceConfig,
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
            inner: Arc::new(SpaceInner { space_dir, config }),
        })
    }

    pub fn root(&self) -> PathBuf {
        SpaceConfig::space_root(&self.inner.space_dir)
    }

    pub fn config(&self) -> &SpaceConfig {
        &self.inner.config
    }
}
