use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

use crate::error::{AppError, AppResult};

/// `.space.toml` — persisted alongside the encrypted space. Plaintext.
/// Contains everything needed to verify a passphrase and identify the owner;
/// no key material.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SpaceConfig {
    pub owner: String,
    pub salt_verify_hex: String,
    pub verifier_hash_hex: String,
    /// scrypt log2(N). Default 15 (N = 32768).
    pub kdf_log_n: u8,
    pub kdf_r: u32,
    pub kdf_p: u32,
}

impl SpaceConfig {
    pub fn config_path(space_dir: &Path) -> PathBuf {
        space_dir.join(".space.toml")
    }

    pub fn space_root(space_dir: &Path) -> PathBuf {
        space_dir.join("space")
    }

    pub fn load(space_dir: &Path) -> AppResult<Self> {
        let path = Self::config_path(space_dir);
        let text = std::fs::read_to_string(&path).map_err(|e| match e.kind() {
            std::io::ErrorKind::NotFound => {
                AppError::BadRequest(format!("no space initialised at {}", space_dir.display()))
            }
            _ => AppError::Io(e),
        })?;
        toml::from_str(&text).map_err(|e| AppError::Internal(format!("parse .space.toml: {e}")))
    }

    pub fn save(&self, space_dir: &Path) -> AppResult<()> {
        let path = Self::config_path(space_dir);
        let text =
            toml::to_string_pretty(self).map_err(|e| AppError::Internal(format!("toml: {e}")))?;
        std::fs::write(path, text)?;
        Ok(())
    }
}
