use age::secrecy::SecretString;

use crate::crypto::age_io;
use crate::error::{AppError, AppResult};
use crate::space::paths::{resolve_under, ENC_EXT};
use crate::space::read::read_file;
use crate::space::write::{write_file, WriteResult};
use crate::space::Space;

/// Restore `path` to its state at `commit_oid` as a forward revert: read the
/// encrypted blob from that commit, decrypt it, and `write_file` a fresh commit
/// on top of HEAD. History stays linear and reversible, and the restore goes
/// through the normal commit cache + meta-index invalidation.
///
/// Restoring overwrites the working tree, which may hold an un-checkpointed
/// autosave draft, so the current draft is snapshotted first and nothing is lost.
pub fn rollback_to(
    space: &Space,
    passphrase: &SecretString,
    rel_path: &str,
    commit_oid: &str,
) -> AppResult<WriteResult> {
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
        age_io::decrypt_bytes(blob.content(), passphrase)
    })?;

    let content = String::from_utf8(plaintext)
        .map_err(|_| AppError::Internal("non-utf8 content in history".into()))?;

    checkpoint_uncommitted_draft(space, passphrase, rel_path)?;

    let short = commit_oid.get(..7).unwrap_or(commit_oid);
    let message = format!("Rollback {rel_path} to {short}");
    write_file(space, passphrase, rel_path, &content, Some(&message))
}

/// If the working-tree copy of `rel_path` differs from what HEAD has committed
/// (i.e. there are un-checkpointed autosave edits), record a checkpoint of the
/// current draft so it remains recoverable from history.
fn checkpoint_uncommitted_draft(
    space: &Space,
    passphrase: &SecretString,
    rel_path: &str,
) -> AppResult<()> {
    let current = match read_file(space, passphrase, rel_path) {
        Ok(file) => file.content,
        Err(AppError::NotFound) => return Ok(()),
        Err(e) => return Err(e),
    };
    if head_content(space, passphrase, rel_path)?.as_deref() == Some(current.as_str()) {
        return Ok(());
    }
    let message = format!("Checkpoint {rel_path} before restore");
    write_file(space, passphrase, rel_path, &current, Some(&message))?;
    Ok(())
}

/// The plaintext of `rel_path` as committed at HEAD, or `None` if HEAD has no
/// commit or the path isn't present there yet.
fn head_content(
    space: &Space,
    passphrase: &SecretString,
    rel_path: &str,
) -> AppResult<Option<String>> {
    let target = format!("{rel_path}{ENC_EXT}");
    space.with_repo(|repo| {
        let commit = match repo.head() {
            Ok(head) => match head.peel_to_commit() {
                Ok(c) => c,
                Err(_) => return Ok(None),
            },
            Err(_) => return Ok(None),
        };
        let tree = commit
            .tree()
            .map_err(|e| AppError::Internal(format!("commit tree: {e}")))?;
        let entry = match tree.get_path(std::path::Path::new(&target)) {
            Ok(e) => e,
            Err(_) => return Ok(None),
        };
        let object = entry
            .to_object(repo)
            .map_err(|e| AppError::Internal(format!("entry to object: {e}")))?;
        let Some(blob) = object.as_blob() else {
            return Ok(None);
        };
        let plaintext = age_io::decrypt_bytes(blob.content(), passphrase)?;
        let content = String::from_utf8(plaintext)
            .map_err(|_| AppError::Internal("non-utf8 content in history".into()))?;
        Ok(Some(content))
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::space::history::file_history;
    use crate::space::test_helpers::make_space;
    use crate::space::{read, write};

    #[test]
    fn rolls_back_to_previous_version() {
        let (_dir, space, pass) = make_space("p");
        write::write_file(&space, &pass, "n.md", "v1", None).unwrap();
        write::write_file(&space, &pass, "n.md", "v2", None).unwrap();
        write::write_file(&space, &pass, "n.md", "v3", None).unwrap();

        let history = file_history(&space, "n.md").unwrap();
        assert_eq!(history.len(), 3);
        let v1_oid = &history[2].commit;

        rollback_to(&space, &pass, "n.md", v1_oid).unwrap();
        let restored = read::read_file(&space, &pass, "n.md").unwrap();
        assert_eq!(restored.content, "v1");

        let after = file_history(&space, "n.md").unwrap();
        assert_eq!(after.len(), 4, "rollback adds a fresh commit");
        assert!(after[0].message.starts_with("Rollback n.md to "));
    }

    #[test]
    fn rollback_preserves_an_uncheckpointed_draft() {
        let (_dir, space, pass) = make_space("p");
        write::write_file(&space, &pass, "n.md", "v1", Some("v1")).unwrap();
        write::save_draft(&space, &pass, "n.md", "draft-v2").unwrap();

        let before = file_history(&space, "n.md").unwrap();
        assert_eq!(
            before.len(),
            1,
            "history knows the checkpoint, not the draft"
        );
        let v1_oid = before[0].commit.clone();

        rollback_to(&space, &pass, "n.md", &v1_oid).unwrap();
        assert_eq!(
            read::read_file(&space, &pass, "n.md").unwrap().content,
            "v1"
        );

        let after = file_history(&space, "n.md").unwrap();
        assert_eq!(after.len(), 3);
        assert!(after[0].message.starts_with("Rollback n.md to "));
        assert!(after[1].message.contains("before restore"));

        let draft_oid = after[1].commit.clone();
        rollback_to(&space, &pass, "n.md", &draft_oid).unwrap();
        assert_eq!(
            read::read_file(&space, &pass, "n.md").unwrap().content,
            "draft-v2"
        );
    }

    #[test]
    fn rollback_with_clean_working_tree_adds_no_extra_checkpoint() {
        let (_dir, space, pass) = make_space("p");
        write::write_file(&space, &pass, "n.md", "v1", Some("v1")).unwrap();
        write::write_file(&space, &pass, "n.md", "v2", Some("v2")).unwrap();
        let history = file_history(&space, "n.md").unwrap();
        let v1_oid = history[1].commit.clone();
        rollback_to(&space, &pass, "n.md", &v1_oid).unwrap();
        let after = file_history(&space, "n.md").unwrap();
        assert_eq!(after.len(), 3);
        assert!(after[0].message.starts_with("Rollback n.md to "));
        assert!(after[1].message == "v2");
    }

    #[test]
    fn rollback_to_head_is_a_noop_visible_only_as_a_new_commit() {
        let (_dir, space, pass) = make_space("p");
        write::write_file(&space, &pass, "n.md", "only", None).unwrap();
        let history = file_history(&space, "n.md").unwrap();
        let head = &history[0].commit;

        rollback_to(&space, &pass, "n.md", head).unwrap();
        let restored = read::read_file(&space, &pass, "n.md").unwrap();
        assert_eq!(restored.content, "only");
    }

    #[test]
    fn rollback_rejects_invalid_oid() {
        let (_dir, space, pass) = make_space("p");
        write::write_file(&space, &pass, "n.md", "x", None).unwrap();
        let err = rollback_to(&space, &pass, "n.md", "not-a-hash").unwrap_err();
        assert!(matches!(err, AppError::BadRequest(_)));
    }

    #[test]
    fn rollback_rejects_unknown_commit() {
        let (_dir, space, pass) = make_space("p");
        write::write_file(&space, &pass, "n.md", "x", None).unwrap();
        let err = rollback_to(&space, &pass, "n.md", &"0".repeat(40)).unwrap_err();
        assert!(matches!(err, AppError::NotFound));
    }

    #[test]
    fn rollback_rejects_commit_without_the_target_file() {
        let (_dir, space, pass) = make_space("p");
        write::write_file(&space, &pass, "a.md", "x", None).unwrap();
        let a_history = file_history(&space, "a.md").unwrap();
        let a_oid = &a_history[0].commit;
        let err = rollback_to(&space, &pass, "b.md", a_oid).unwrap_err();
        assert!(matches!(err, AppError::NotFound));
    }

    #[test]
    fn rollback_blocks_path_traversal() {
        let (_dir, space, pass) = make_space("p");
        write::write_file(&space, &pass, "n.md", "x", None).unwrap();
        let history = file_history(&space, "n.md").unwrap();
        let err = rollback_to(&space, &pass, "../escape", &history[0].commit).unwrap_err();
        assert!(matches!(err, AppError::Forbidden));
    }
}
