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
