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

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    fn init_repo() -> TempDir {
        let dir = TempDir::new().unwrap();
        git2::Repository::init(dir.path()).unwrap();
        dir
    }

    fn count_commits(repo_path: &Path) -> usize {
        let repo = git2::Repository::open(repo_path).unwrap();
        let mut walk = repo.revwalk().unwrap();
        if walk.push_head().is_err() {
            return 0;
        }
        walk.filter_map(Result::ok).count()
    }

    #[test]
    fn first_commit_creates_root_commit() {
        let dir = init_repo();
        std::fs::write(dir.path().join("first.txt"), b"hello").unwrap();
        commit_all(dir.path(), "initial").unwrap();
        assert_eq!(count_commits(dir.path()), 1);
    }

    #[test]
    fn second_commit_extends_the_history() {
        let dir = init_repo();
        std::fs::write(dir.path().join("a.txt"), b"a").unwrap();
        commit_all(dir.path(), "a").unwrap();
        std::fs::write(dir.path().join("b.txt"), b"b").unwrap();
        commit_all(dir.path(), "b").unwrap();
        assert_eq!(count_commits(dir.path()), 2);
    }

    #[test]
    fn commit_uses_author_metadata() {
        let dir = init_repo();
        std::fs::write(dir.path().join("x"), b"x").unwrap();
        commit_all(dir.path(), "msg").unwrap();
        let repo = git2::Repository::open(dir.path()).unwrap();
        let head = repo.head().unwrap().peel_to_commit().unwrap();
        assert_eq!(head.author().name(), Some("hearth"));
        assert_eq!(head.author().email(), Some("hearth@local"));
        assert_eq!(head.message().unwrap().trim(), "msg");
    }

    #[test]
    fn commit_picks_up_a_new_file() {
        let dir = init_repo();
        std::fs::write(dir.path().join("first.txt"), b"a").unwrap();
        commit_all(dir.path(), "first").unwrap();
        std::fs::write(dir.path().join("second.txt"), b"b").unwrap();
        commit_all(dir.path(), "second").unwrap();
        let repo = git2::Repository::open(dir.path()).unwrap();
        let head = repo.head().unwrap().peel_to_tree().unwrap();
        assert!(head.get_path(Path::new("first.txt")).is_ok());
        assert!(head.get_path(Path::new("second.txt")).is_ok());
    }

    #[test]
    fn commit_on_missing_repo_errors() {
        let dir = TempDir::new().unwrap();
        // No git2::Repository::init — should fail open.
        let err = commit_all(dir.path(), "x").unwrap_err();
        assert!(matches!(err, AppError::Internal(_)));
    }
}
