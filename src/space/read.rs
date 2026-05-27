use std::time::SystemTime;

use age::secrecy::SecretString;
use time::OffsetDateTime;

use crate::crypto::age_io;
use crate::error::{AppError, AppResult};
use crate::space::paths::{resolve_under, with_age_suffix};
use crate::space::Space;

pub struct ReadFile {
    pub path: String,
    pub content: String,
    pub updated: Option<String>,
}

pub fn read_file(space: &Space, passphrase: &SecretString, rel_path: &str) -> AppResult<ReadFile> {
    let resolved = resolve_under(&space.root(), rel_path)?;
    let on_disk = with_age_suffix(&resolved);
    if !on_disk.is_file() {
        return Err(AppError::NotFound);
    }
    let bytes = std::fs::read(&on_disk)?;
    let plaintext = age_io::decrypt_bytes(&bytes, passphrase)?;
    let content = String::from_utf8(plaintext)
        .map_err(|_| AppError::Internal("non-utf8 note content".into()))?;
    let updated = std::fs::metadata(&on_disk)
        .and_then(|m| m.modified())
        .ok()
        .and_then(systemtime_iso8601);
    Ok(ReadFile {
        path: rel_path.to_string(),
        content,
        updated,
    })
}

fn systemtime_iso8601(t: SystemTime) -> Option<String> {
    let dt: OffsetDateTime = t.into();
    dt.format(&time::format_description::well_known::Rfc3339).ok()
}
