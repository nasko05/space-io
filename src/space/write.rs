use age::secrecy::SecretString;

use crate::crypto::age_io;
use crate::error::AppResult;
use crate::space::git::commit_all;
use crate::space::paths::{resolve_under, with_age_suffix};
use crate::space::Space;

#[derive(Debug)]
pub struct WriteResult {
    pub path: String,
    pub updated: String,
}

pub fn write_file(
    space: &Space,
    passphrase: &SecretString,
    rel_path: &str,
    content: &str,
    message: Option<&str>,
) -> AppResult<WriteResult> {
    let root = space.root();
    let resolved = resolve_under(&root, rel_path)?;
    if let Some(parent) = resolved.parent() {
        std::fs::create_dir_all(parent)?;
    }
    let on_disk = with_age_suffix(&resolved);

    let ciphertext = age_io::encrypt_bytes(content.as_bytes(), passphrase)?;
    std::fs::write(&on_disk, &ciphertext)?;

    let summary = message
        .map(|m| m.to_string())
        .unwrap_or_else(|| format!("Edit: {rel_path}"));
    commit_all(&root, &summary)?;

    let updated = std::fs::metadata(&on_disk)
        .and_then(|m| m.modified())
        .ok()
        .and_then(|t| {
            let dt: time::OffsetDateTime = t.into();
            dt.format(&time::format_description::well_known::Rfc3339)
                .ok()
        })
        .unwrap_or_default();

    Ok(WriteResult {
        path: rel_path.to_string(),
        updated,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::error::AppError;
    use crate::space::test_helpers::make_space;

    fn count_commits(root: &std::path::Path) -> usize {
        let repo = git2::Repository::open(root).unwrap();
        let mut walk = repo.revwalk().unwrap();
        if walk.push_head().is_err() {
            return 0;
        }
        walk.filter_map(Result::ok).count()
    }

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
