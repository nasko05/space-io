use age::secrecy::SecretString;

use crate::crypto::age_io;
use crate::error::{AppError, AppResult};
use crate::space::paths::{resolve_under, ENC_EXT};
use crate::space::write::{write_file, WriteResult};
use crate::space::Space;

/// Restore `path` to the state it had at `commit_oid`. Implemented as a
/// "forward revert": we read the encrypted blob from that commit, decrypt it
/// with the current passphrase, and call `write_file` so a fresh commit
/// records the restoration on top of HEAD. That keeps the history linear and
/// reversible (no `reset --hard`-style destruction), and means the rollback
/// participates in the normal commit cache + meta-index invalidation.
pub fn rollback_to(
    space: &Space,
    passphrase: &SecretString,
    rel_path: &str,
    commit_oid: &str,
) -> AppResult<WriteResult> {
    // Validate the path up front; resolve_under returns Forbidden on
    // traversal so a bad commit doesn't get us a foothold to read elsewhere.
    let root = space.root();
    resolve_under(&root, rel_path)?;

    let target = format!("{rel_path}{ENC_EXT}");
    let plaintext = space.with_repo(|repo| {
        let oid = git2::Oid::from_str(commit_oid)
            .map_err(|_| AppError::BadRequest("invalid commit id".into()))?;
        let commit = repo.find_commit(oid).map_err(|_| AppError::NotFound)?;
        let tree = commit
            .tree()
            .map_err(|e| AppError::Internal(format!("commit tree: {e}")))?;
        let entry = tree
            .get_path(std::path::Path::new(&target))
            .map_err(|_| AppError::NotFound)?;
        let object = entry
            .to_object(repo)
            .map_err(|e| AppError::Internal(format!("entry to object: {e}")))?;
        let blob = object
            .as_blob()
            .ok_or_else(|| AppError::BadRequest("history entry is not a file".into()))?;
        // Decrypt while we still own the blob borrow; bubbling the error up
        // here surfaces "wrong passphrase" as 401 rather than a 500.
        age_io::decrypt_bytes(blob.content(), passphrase)
    })?;

    let content = String::from_utf8(plaintext)
        .map_err(|_| AppError::Internal("non-utf8 content in history".into()))?;

    let short = commit_oid.get(..7).unwrap_or(commit_oid);
    let message = format!("Rollback {rel_path} to {short}");
    write_file(space, passphrase, rel_path, &content, Some(&message))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::space::history::file_history;
    use crate::space::test_helpers::make_space;
    use crate::space::{read, write};

    #[test]
    fn rolls_back_to_previous_version() {
        let (_d, s, p) = make_space("p");
        write::write_file(&s, &p, "n.md", "v1", None).unwrap();
        write::write_file(&s, &p, "n.md", "v2", None).unwrap();
        write::write_file(&s, &p, "n.md", "v3", None).unwrap();

        let history = file_history(&s, "n.md").unwrap();
        assert_eq!(history.len(), 3);
        // history[0] is v3 (HEAD); history[2] is v1.
        let v1_oid = &history[2].commit;

        rollback_to(&s, &p, "n.md", v1_oid).unwrap();
        let restored = read::read_file(&s, &p, "n.md").unwrap();
        assert_eq!(restored.content, "v1");

        // A fresh commit was created — history is now 4 entries deep.
        let after = file_history(&s, "n.md").unwrap();
        assert_eq!(after.len(), 4);
        assert!(after[0].message.starts_with("Rollback n.md to "));
    }

    #[test]
    fn rollback_to_head_is_a_noop_visible_only_as_a_new_commit() {
        let (_d, s, p) = make_space("p");
        write::write_file(&s, &p, "n.md", "only", None).unwrap();
        let history = file_history(&s, "n.md").unwrap();
        let head = &history[0].commit;

        rollback_to(&s, &p, "n.md", head).unwrap();
        let restored = read::read_file(&s, &p, "n.md").unwrap();
        assert_eq!(restored.content, "only");
    }

    #[test]
    fn rollback_rejects_invalid_oid() {
        let (_d, s, p) = make_space("p");
        write::write_file(&s, &p, "n.md", "x", None).unwrap();
        let err = rollback_to(&s, &p, "n.md", "not-a-hash").unwrap_err();
        assert!(matches!(err, AppError::BadRequest(_)));
    }

    #[test]
    fn rollback_rejects_unknown_commit() {
        let (_d, s, p) = make_space("p");
        write::write_file(&s, &p, "n.md", "x", None).unwrap();
        let err = rollback_to(&s, &p, "n.md", &"0".repeat(40)).unwrap_err();
        assert!(matches!(err, AppError::NotFound));
    }

    #[test]
    fn rollback_rejects_commit_without_the_target_file() {
        let (_d, s, p) = make_space("p");
        write::write_file(&s, &p, "a.md", "x", None).unwrap();
        let a_history = file_history(&s, "a.md").unwrap();
        let a_oid = &a_history[0].commit;
        // b.md was never in that commit's tree.
        let err = rollback_to(&s, &p, "b.md", a_oid).unwrap_err();
        assert!(matches!(err, AppError::NotFound));
    }

    #[test]
    fn rollback_blocks_path_traversal() {
        let (_d, s, p) = make_space("p");
        write::write_file(&s, &p, "n.md", "x", None).unwrap();
        let h = file_history(&s, "n.md").unwrap();
        let err = rollback_to(&s, &p, "../escape", &h[0].commit).unwrap_err();
        assert!(matches!(err, AppError::Forbidden));
    }
}
