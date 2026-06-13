//! Multi-tenant user registry: a plain-TOML `email -> uuid` mapping at
//! `<root>/.users.toml`. Each user owns a sibling `<root>/<uuid>/` directory
//! holding their `.space.toml` and `space/` git repo.
//!
//! No database — the file is read per request and rewritten on mutation.
//! Mutations are serialised by the caller's `RwLock` and persisted atomically
//! via `fs_atomic::write_atomic`, so a crash mid-write can't tear the registry.

use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};
use time::format_description::well_known::Rfc3339;
use time::OffsetDateTime;
use uuid::Uuid;

use crate::error::{AppError, AppResult};

pub const REGISTRY_FILENAME: &str = ".users.toml";

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct UserEntry {
    pub email: String,
    pub uuid: Uuid,
    /// RFC 3339. Stored as a string so the file is human-editable.
    pub created_at: String,
}

#[derive(Debug, Default, Serialize, Deserialize)]
struct OnDisk {
    #[serde(default, rename = "users")]
    users: Vec<UserEntry>,
}

#[derive(Debug, Default, Clone)]
pub struct UsersRegistry {
    pub users: Vec<UserEntry>,
}

impl UsersRegistry {
    pub fn registry_path(root: &Path) -> PathBuf {
        root.join(REGISTRY_FILENAME)
    }

    /// Resolve the per-user space directory: `<root>/<uuid>/`.
    pub fn space_dir_for(root: &Path, uuid: &Uuid) -> PathBuf {
        root.join(uuid.to_string())
    }

    /// Load from `<root>/.users.toml`. Returns an empty registry if the file
    /// doesn't exist — first-run state.
    pub fn load(root: &Path) -> AppResult<Self> {
        let path = Self::registry_path(root);
        match std::fs::read_to_string(&path) {
            Ok(text) => {
                let parsed: OnDisk = toml::from_str(&text)
                    .map_err(|e| AppError::Internal(format!("parse {REGISTRY_FILENAME}: {e}")))?;
                Ok(Self {
                    users: parsed.users,
                })
            }
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(Self::default()),
            Err(e) => Err(AppError::Io(e)),
        }
    }

    pub fn save(&self, root: &Path) -> AppResult<()> {
        let path = Self::registry_path(root);
        let on_disk = OnDisk {
            users: self.users.clone(),
        };
        let text = toml::to_string_pretty(&on_disk)
            .map_err(|e| AppError::Internal(format!("serialise {REGISTRY_FILENAME}: {e}")))?;
        // Atomic: a torn `.users.toml` would lose the email→UUID map and make
        // every space unreachable.
        crate::fs_atomic::write_atomic(&path, text.as_bytes())?;
        Ok(())
    }

    pub fn is_empty(&self) -> bool {
        self.users.is_empty()
    }

    pub fn find_by_email(&self, email: &str) -> Option<&UserEntry> {
        let needle = email.trim().to_ascii_lowercase();
        self.users.iter().find(|user| user.email == needle)
    }

    /// Drop the entry whose `uuid` matches; used by registration rollback when
    /// `init_space` fails after the entry was added.
    pub fn remove_by_uuid(&mut self, uuid: &Uuid) {
        self.users.retain(|user| user.uuid != *uuid);
    }

    /// Register a new user: mint a UUID, append, and return the entry. The
    /// caller creates the per-user subdirectory and runs `init_space` in it.
    pub fn add(&mut self, root: &Path, email: &str) -> AppResult<UserEntry> {
        let normalised = normalise_email(email)?;
        if self.find_by_email(&normalised).is_some() {
            return Err(AppError::BadRequest(format!(
                "an account for {normalised} already exists"
            )));
        }
        // v4 collisions are astronomically unlikely, but a broken RNG or
        // corrupted file could surface one; regenerate defensively.
        let mut uuid = Uuid::new_v4();
        for _ in 0..16 {
            if !self.users.iter().any(|user| user.uuid == uuid)
                && !UsersRegistry::space_dir_for(root, &uuid).exists()
            {
                break;
            }
            uuid = Uuid::new_v4();
        }
        let entry = UserEntry {
            email: normalised,
            uuid,
            created_at: OffsetDateTime::now_utc()
                .format(&Rfc3339)
                .unwrap_or_else(|_| "1970-01-01T00:00:00Z".into()),
        };
        self.users.push(entry.clone());
        self.save(root)?;
        Ok(entry)
    }
}

/// Light validation: must contain `@` with non-empty halves and no whitespace.
/// Not a full RFC 5322 check — the email is just an opaque identifier here, so
/// it only needs to look email-shaped.
pub fn normalise_email(email: &str) -> AppResult<String> {
    let trimmed = email.trim();
    if trimmed.is_empty() {
        return Err(AppError::BadRequest("email must not be empty".into()));
    }
    if trimmed.chars().any(char::is_whitespace) {
        return Err(AppError::BadRequest(
            "email must not contain whitespace".into(),
        ));
    }
    let at = trimmed
        .find('@')
        .ok_or_else(|| AppError::BadRequest("email must contain '@'".into()))?;
    let (local, domain) = trimmed.split_at(at);
    let domain = &domain[1..];
    if local.is_empty() || domain.is_empty() || !domain.contains('.') {
        return Err(AppError::BadRequest(
            "email must look like name@host.example".into(),
        ));
    }
    Ok(trimmed.to_ascii_lowercase())
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    #[test]
    fn empty_registry_when_file_missing() {
        let dir = TempDir::new().unwrap();
        let registry = UsersRegistry::load(dir.path()).unwrap();
        assert!(registry.is_empty());
    }

    #[test]
    fn add_persists_and_round_trips() {
        let dir = TempDir::new().unwrap();
        let mut registry = UsersRegistry::load(dir.path()).unwrap();
        let added = registry.add(dir.path(), "Alice@example.com").unwrap();
        assert_eq!(added.email, "alice@example.com");
        let reloaded = UsersRegistry::load(dir.path()).unwrap();
        assert_eq!(reloaded.users.len(), 1);
        assert_eq!(reloaded.users[0].uuid, added.uuid);
    }

    #[test]
    fn duplicate_email_rejected_case_insensitive() {
        let dir = TempDir::new().unwrap();
        let mut registry = UsersRegistry::load(dir.path()).unwrap();
        registry.add(dir.path(), "alice@example.com").unwrap();
        let err = registry.add(dir.path(), "ALICE@example.com").unwrap_err();
        assert!(matches!(err, AppError::BadRequest(_)));
    }

    #[test]
    fn find_is_case_insensitive() {
        let dir = TempDir::new().unwrap();
        let mut registry = UsersRegistry::load(dir.path()).unwrap();
        let added = registry.add(dir.path(), "alice@example.com").unwrap();
        let found = registry.find_by_email("ALICE@example.com").unwrap();
        assert_eq!(found.uuid, added.uuid);
    }

    #[test]
    fn space_dir_for_uses_uuid_under_root() {
        let uuid = Uuid::new_v4();
        let dir = UsersRegistry::space_dir_for(Path::new("/data"), &uuid);
        assert_eq!(dir, Path::new("/data").join(uuid.to_string()));
    }

    #[test]
    fn normalise_rejects_blanks_and_bad_shapes() {
        assert!(normalise_email("").is_err());
        assert!(normalise_email("plain").is_err());
        assert!(normalise_email("@host.com").is_err());
        assert!(normalise_email("a@b").is_err());
        assert!(normalise_email("a @b.c").is_err());
        assert_eq!(normalise_email(" Ada@Home.Lan ").unwrap(), "ada@home.lan");
    }

    #[test]
    fn malformed_file_yields_internal_error() {
        let dir = TempDir::new().unwrap();
        std::fs::write(
            UsersRegistry::registry_path(dir.path()),
            "not toml at all {",
        )
        .unwrap();
        let err = UsersRegistry::load(dir.path()).unwrap_err();
        assert!(matches!(err, AppError::Internal(_)));
    }
}
