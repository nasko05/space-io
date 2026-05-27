use std::path::Path;

use crate::error::{AppError, AppResult};

/// Stage everything under `repo_path` and create a commit. Used by `init`,
/// `write`, and `create` to keep the space as a git repository where every
/// edit is a commit.
pub fn commit_all(repo_path: &Path, message: &str) -> AppResult<()> {
    let repo = git2::Repository::open(repo_path)
        .map_err(|e| AppError::Internal(format!("git open: {e}")))?;
    let mut index = repo
        .index()
        .map_err(|e| AppError::Internal(format!("git index: {e}")))?;
    index
        .add_all(["*"].iter(), git2::IndexAddOption::DEFAULT, None)
        .map_err(|e| AppError::Internal(format!("git add: {e}")))?;
    index
        .write()
        .map_err(|e| AppError::Internal(format!("git index write: {e}")))?;
    let tree_oid = index
        .write_tree()
        .map_err(|e| AppError::Internal(format!("git write_tree: {e}")))?;
    let tree = repo
        .find_tree(tree_oid)
        .map_err(|e| AppError::Internal(format!("git find_tree: {e}")))?;
    let sig = git2::Signature::now("hearth", "hearth@local")
        .map_err(|e| AppError::Internal(format!("git signature: {e}")))?;
    let parents: Vec<git2::Commit> = match repo.head() {
        Ok(head) => head
            .peel_to_commit()
            .ok()
            .map(|c| vec![c])
            .unwrap_or_default(),
        Err(_) => vec![],
    };
    let parent_refs: Vec<&git2::Commit> = parents.iter().collect();
    repo.commit(Some("HEAD"), &sig, &sig, message, &tree, &parent_refs)
        .map_err(|e| AppError::Internal(format!("git commit: {e}")))?;
    Ok(())
}
