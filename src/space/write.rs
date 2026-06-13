use std::path::{Path, PathBuf};

use age::secrecy::SecretString;

use crate::crypto::age_io;
use crate::error::AppResult;
use crate::space::git::commit_paths;
use crate::space::paths::{relative_to, resolve_under, with_age_suffix};
use crate::space::{systemtime_iso8601, Space};

#[derive(Debug)]
pub struct WriteResult {
    pub path: String,
    pub updated: String,
}

/// Persist `content` to the working tree without a git commit (the autosave
/// path). Edits flow to disk continuously so nothing is lost on reload or crash,
/// but only an explicit [`write_file`] checkpoint mints a history entry.
pub fn save_draft(
    space: &Space,
    passphrase: &SecretString,
    rel_path: &str,
    content: &str,
) -> AppResult<WriteResult> {
    let on_disk = persist_encrypted(space, passphrase, rel_path, content)?;
    Ok(WriteResult {
        path: rel_path.to_string(),
        updated: modified_iso8601(&on_disk),
    })
}

/// Persist `content` to the working tree and record a commit (checkpoint). Used
/// by explicit checkpoints, rollback, and the AI assistant.
pub fn write_file(
    space: &Space,
    passphrase: &SecretString,
    rel_path: &str,
    content: &str,
    message: Option<&str>,
) -> AppResult<WriteResult> {
    let root = space.root();
    let on_disk = persist_encrypted(space, passphrase, rel_path, content)?;

    let summary = message
        .map(|m| m.to_string())
        .unwrap_or_else(|| format!("Edit: {rel_path}"));
    let staged = relative_to(&root, &on_disk);
    space.with_repo(|repo| commit_paths(repo, &summary, [staged]))?;

    Ok(WriteResult {
        path: rel_path.to_string(),
        updated: modified_iso8601(&on_disk),
    })
}

/// Encrypt `content` to `<rel_path>.age` in the working tree, creating parent
/// directories and invalidating the decrypted cache. Returns the absolute
/// on-disk path. Shared by [`save_draft`] and [`write_file`] so they can't drift
/// in how bytes hit disk.
fn persist_encrypted(
    space: &Space,
    passphrase: &SecretString,
    rel_path: &str,
    content: &str,
) -> AppResult<PathBuf> {
    let root = space.root();
    let resolved = resolve_under(&root, rel_path)?;
    if let Some(parent) = resolved.parent() {
        std::fs::create_dir_all(parent)?;
    }
    let on_disk = with_age_suffix(&resolved);

    let ciphertext = age_io::encrypt_bytes(content.as_bytes(), passphrase)?;
    std::fs::write(&on_disk, &ciphertext)?;
    space.cache().invalidate(&on_disk.to_string_lossy());
    Ok(on_disk)
}

fn modified_iso8601(on_disk: &Path) -> String {
    std::fs::metadata(on_disk)
        .and_then(|m| m.modified())
        .ok()
        .and_then(systemtime_iso8601)
        .unwrap_or_default()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::error::AppError;
    use crate::space::test_helpers::{count_commits, make_space};

    #[test]
    fn writes_an_encrypted_blob_at_the_expected_path() {
        let (dir, space, pass) = make_space("p");
        write_file(&space, &pass, "Journal/2026/note.md", "hello", None).unwrap();
        let on_disk = dir.path().join("space").join("Journal/2026/note.md.age");
        assert!(on_disk.is_file());
        let bytes = std::fs::read(&on_disk).unwrap();
        assert!(bytes.starts_with(b"age-encryption.org/v1\n"));
    }

    #[test]
    fn write_produces_a_git_commit() {
        let (dir, space, pass) = make_space("p");
        assert_eq!(count_commits(&dir.path().join("space")), 0);
        write_file(&space, &pass, "Journal/2026/note.md", "x", None).unwrap();
        assert_eq!(count_commits(&dir.path().join("space")), 1);
    }

    #[test]
    fn write_uses_supplied_commit_message() {
        let (dir, space, pass) = make_space("p");
        write_file(&space, &pass, "Journal/2026/n.md", "x", Some("hand-rolled")).unwrap();
        let repo = git2::Repository::open(dir.path().join("space")).unwrap();
        let head = repo.head().unwrap().peel_to_commit().unwrap();
        assert_eq!(head.message().unwrap().trim(), "hand-rolled");
    }

    #[test]
    fn default_commit_message_includes_path() {
        let (dir, space, pass) = make_space("p");
        write_file(&space, &pass, "Journal/2026/n.md", "x", None).unwrap();
        let repo = git2::Repository::open(dir.path().join("space")).unwrap();
        let head = repo.head().unwrap().peel_to_commit().unwrap();
        assert!(head.message().unwrap().contains("Journal/2026/n.md"));
    }

    #[test]
    fn rewriting_the_same_path_produces_a_second_commit() {
        let (dir, space, pass) = make_space("p");
        write_file(&space, &pass, "n.md", "v1", None).unwrap();
        write_file(&space, &pass, "n.md", "v2", None).unwrap();
        assert_eq!(count_commits(&dir.path().join("space")), 2);
    }

    #[test]
    fn save_draft_writes_an_encrypted_blob() {
        let (dir, space, pass) = make_space("p");
        save_draft(&space, &pass, "Journal/2026/note.md", "hello").unwrap();
        let on_disk = dir.path().join("space").join("Journal/2026/note.md.age");
        assert!(on_disk.is_file());
        let bytes = std::fs::read(&on_disk).unwrap();
        assert!(bytes.starts_with(b"age-encryption.org/v1\n"));
    }

    #[test]
    fn save_draft_does_not_commit() {
        let (dir, space, pass) = make_space("p");
        assert_eq!(count_commits(&dir.path().join("space")), 0);
        save_draft(&space, &pass, "n.md", "draft one").unwrap();
        save_draft(&space, &pass, "n.md", "draft two").unwrap();
        assert_eq!(count_commits(&dir.path().join("space")), 0);
    }

    #[test]
    fn save_draft_then_read_round_trips_the_latest_draft() {
        let (dir, space, pass) = make_space("p");
        save_draft(&space, &pass, "n.md", "first").unwrap();
        save_draft(&space, &pass, "n.md", "second").unwrap();
        let bytes = std::fs::read(dir.path().join("space/n.md.age")).unwrap();
        let pt = crate::crypto::age_io::decrypt_bytes(&bytes, &pass).unwrap();
        assert_eq!(pt, b"second");
    }

    #[test]
    fn checkpoint_after_drafts_records_a_single_commit() {
        let (dir, space, pass) = make_space("p");
        save_draft(&space, &pass, "n.md", "wip 1").unwrap();
        save_draft(&space, &pass, "n.md", "wip 2").unwrap();
        assert_eq!(count_commits(&dir.path().join("space")), 0);
        write_file(&space, &pass, "n.md", "wip 2", Some("checkpoint")).unwrap();
        assert_eq!(count_commits(&dir.path().join("space")), 1);
    }

    #[test]
    fn save_draft_rejects_path_traversal() {
        let (_dir, space, pass) = make_space("p");
        let err = save_draft(&space, &pass, "../etc/x", "x").unwrap_err();
        assert!(matches!(err, AppError::Forbidden));
    }

    #[test]
    fn rejects_path_traversal() {
        let (_dir, space, pass) = make_space("p");
        let err = write_file(&space, &pass, "../etc/x", "x", None).unwrap_err();
        assert!(matches!(err, AppError::Forbidden));
    }

    #[test]
    fn creates_intermediate_directories() {
        let (dir, space, pass) = make_space("p");
        write_file(&space, &pass, "a/deep/path/n.md", "x", None).unwrap();
        assert!(dir.path().join("space/a/deep/path/n.md.age").is_file());
    }

    #[test]
    fn empty_content_writes_a_valid_encrypted_blob() {
        let (dir, space, pass) = make_space("p");
        write_file(&space, &pass, "n.md", "", None).unwrap();
        let bytes = std::fs::read(dir.path().join("space/n.md.age")).unwrap();
        let pt = crate::crypto::age_io::decrypt_bytes(&bytes, &pass).unwrap();
        assert!(pt.is_empty());
    }
}
