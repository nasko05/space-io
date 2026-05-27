use std::time::SystemTime;

use age::secrecy::SecretString;
use time::OffsetDateTime;

use crate::crypto::age_io;
use crate::error::{AppError, AppResult};
use crate::space::paths::{resolve_under, with_age_suffix};
use crate::space::Space;

#[derive(Debug)]
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
    dt.format(&time::format_description::well_known::Rfc3339)
        .ok()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::space::test_helpers::make_space_with_note;

    #[test]
    fn reads_a_note_decrypted() {
        let (_dir, space, pass) =
            make_space_with_note("p", "Journal/2026/note.md", "# Title\n\nBody.");
        let r = read_file(&space, &pass, "Journal/2026/note.md").unwrap();
        assert_eq!(r.path, "Journal/2026/note.md");
        assert_eq!(r.content, "# Title\n\nBody.");
        assert!(r.updated.is_some());
    }

    #[test]
    fn missing_file_yields_not_found() {
        let (_dir, space, pass) = make_space_with_note("p", "Journal/2026/note.md", "x");
        let err = read_file(&space, &pass, "Journal/2026/missing.md").unwrap_err();
        assert!(matches!(err, AppError::NotFound));
    }

    #[test]
    fn traversal_yields_forbidden() {
        let (_dir, space, pass) = make_space_with_note("p", "Journal/2026/note.md", "x");
        let err = read_file(&space, &pass, "../../etc/passwd").unwrap_err();
        assert!(matches!(err, AppError::Forbidden));
    }

    #[test]
    fn wrong_passphrase_returns_internal_error() {
        let (_dir, space, _pass) = make_space_with_note("right one", "Journal/2026/note.md", "x");
        let wrong = SecretString::from("wrong one".to_string());
        let err = read_file(&space, &wrong, "Journal/2026/note.md").unwrap_err();
        assert!(matches!(err, AppError::Internal(_)));
    }

    #[test]
    fn returns_iso8601_updated_timestamp() {
        let (_dir, space, pass) = make_space_with_note("p", "Journal/2026/note.md", "x");
        let r = read_file(&space, &pass, "Journal/2026/note.md").unwrap();
        let updated = r.updated.unwrap();
        // RFC3339 has at least the form YYYY-MM-DDTHH:MM:SS+TZ
        assert!(updated.contains('T'), "iso8601: {updated}");
    }
}
