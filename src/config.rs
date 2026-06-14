use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

use crate::error::{AppError, AppResult};

/// Plaintext `.space.toml` persisted alongside the encrypted space. Holds
/// everything needed to verify a passphrase and identify the owner; no key
/// material.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SpaceConfig {
    pub owner: String,
    pub salt_verify_hex: String,
    pub verifier_hash_hex: String,
    /// scrypt log2(N); default 15 (N = 32768).
    pub kdf_log_n: u8,
    pub kdf_r: u32,
    pub kdf_p: u32,
    /// Optional WebAuthn-passkey unlock material. The server stores only opaque
    /// ciphertext + PRF salt; decryption happens in the browser.
    #[serde(default)]
    pub passkey: Option<PasskeyConfig>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PasskeyConfig {
    /// Base64url-encoded WebAuthn credential ID.
    pub credential_id_b64: String,
    /// Base64-encoded 32-byte salt fed to the PRF extension.
    pub prf_salt_b64: String,
    /// Base64-encoded `iv (12B) || ciphertext` of the passphrase, wrapped under
    /// a key derived from the PRF output via HKDF.
    pub wrapped_passphrase_b64: String,
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

    /// Persists `.space.toml` via an atomic write so a crash can't tear the file
    /// and lock the user out of their space.
    pub fn save(&self, space_dir: &Path) -> AppResult<()> {
        let path = Self::config_path(space_dir);
        let text =
            toml::to_string_pretty(self).map_err(|e| AppError::Internal(format!("toml: {e}")))?;
        crate::fs_atomic::write_atomic(&path, text.as_bytes())?;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    fn sample() -> SpaceConfig {
        SpaceConfig {
            owner: "ada@home.lan".into(),
            salt_verify_hex: "deadbeef".into(),
            verifier_hash_hex: "cafebabe".into(),
            kdf_log_n: 15,
            kdf_r: 8,
            kdf_p: 1,
            passkey: None,
        }
    }

    #[test]
    fn config_path_places_inside_space_dir() {
        let p = SpaceConfig::config_path(Path::new("/foo"));
        assert_eq!(p, Path::new("/foo/.space.toml"));
    }

    #[test]
    fn space_root_places_inside_space_dir() {
        let p = SpaceConfig::space_root(Path::new("/foo"));
        assert_eq!(p, Path::new("/foo/space"));
    }

    #[test]
    fn save_then_load_roundtrips() {
        let dir = TempDir::new().unwrap();
        let cfg = sample();
        cfg.save(dir.path()).unwrap();
        let loaded = SpaceConfig::load(dir.path()).unwrap();
        assert_eq!(loaded.owner, cfg.owner);
        assert_eq!(loaded.salt_verify_hex, cfg.salt_verify_hex);
        assert_eq!(loaded.verifier_hash_hex, cfg.verifier_hash_hex);
        assert_eq!(loaded.kdf_log_n, cfg.kdf_log_n);
        assert!(loaded.passkey.is_none());
    }

    #[test]
    fn loading_missing_config_yields_bad_request() {
        let dir = TempDir::new().unwrap();
        let err = SpaceConfig::load(dir.path()).unwrap_err();
        assert!(matches!(err, AppError::BadRequest(_)));
    }

    #[test]
    fn loading_malformed_config_yields_internal_error() {
        let dir = TempDir::new().unwrap();
        std::fs::write(SpaceConfig::config_path(dir.path()), "{ not toml").unwrap();
        let err = SpaceConfig::load(dir.path()).unwrap_err();
        assert!(matches!(err, AppError::Internal(_)));
    }

    #[test]
    fn passkey_round_trips_through_disk() {
        let dir = TempDir::new().unwrap();
        let mut cfg = sample();
        cfg.passkey = Some(PasskeyConfig {
            credential_id_b64: "abc".into(),
            prf_salt_b64: "salt".into(),
            wrapped_passphrase_b64: "wrap".into(),
        });
        cfg.save(dir.path()).unwrap();
        let loaded = SpaceConfig::load(dir.path()).unwrap();
        let pk = loaded.passkey.expect("passkey persisted");
        assert_eq!(pk.credential_id_b64, "abc");
        assert_eq!(pk.prf_salt_b64, "salt");
        assert_eq!(pk.wrapped_passphrase_b64, "wrap");
    }

    #[test]
    fn legacy_config_without_passkey_section_loads() {
        let dir = TempDir::new().unwrap();
        std::fs::write(
            SpaceConfig::config_path(dir.path()),
            r#"
owner = "old@home.lan"
salt_verify_hex = "00"
verifier_hash_hex = "11"
kdf_log_n = 15
kdf_r = 8
kdf_p = 1
"#,
        )
        .unwrap();
        let loaded = SpaceConfig::load(dir.path()).unwrap();
        assert_eq!(loaded.owner, "old@home.lan");
        assert!(loaded.passkey.is_none());
    }
}
