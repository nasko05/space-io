use std::path::Path;

use git2::Repository;

use crate::error::{AppError, AppResult};

/// Stage everything under `repo` and create a commit. Callers come from
/// `init`, `write`, `create`, `delete`, `rename`, `meta`, and `upload` to
/// keep the space as a git repository where every edit is a commit.
///
/// The repository handle is held by `Space` so we don't pay the
/// `Repository::open` cost (scan packs, parse refs) on every write.
pub fn commit_all(repo: &Repository, message: &str) -> AppResult<()> {
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
    // If HEAD resolves to a commit, that's our parent. If HEAD is missing
    // (root commit) we proceed with no parents. Anything else — a HEAD that
    // points at something that won't peel — is a real failure and must
    // bubble up: silently treating it as "no parents" would orphan history.
    let parents: Vec<git2::Commit> = match repo.head() {
        Ok(head) => match head.peel_to_commit() {
            Ok(c) => vec![c],
            Err(e) => return Err(AppError::Internal(format!("git peel head: {e}"))),
        },
        Err(e)
            if e.code() == git2::ErrorCode::UnbornBranch
                || e.code() == git2::ErrorCode::NotFound =>
        {
            vec![]
        }
        Err(e) => return Err(AppError::Internal(format!("git head: {e}"))),
    };
    let parent_refs: Vec<&git2::Commit> = parents.iter().collect();
    repo.commit(Some("HEAD"), &sig, &sig, message, &tree, &parent_refs)
        .map_err(|e| AppError::Internal(format!("git commit: {e}")))?;
    Ok(())
}

/// Open a repository, used at startup to populate the cache in `Space`.
pub fn open(repo_path: &Path) -> AppResult<Repository> {
    Repository::open(repo_path).map_err(|e| AppError::Internal(format!("git open: {e}")))
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    fn init_repo() -> (TempDir, Repository) {
        let dir = TempDir::new().unwrap();
        let repo = git2::Repository::init(dir.path()).unwrap();
        (dir, repo)
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
        let (dir, repo) = init_repo();
        std::fs::write(dir.path().join("first.txt"), b"hello").unwrap();
        commit_all(&repo, "initial").unwrap();
        assert_eq!(count_commits(dir.path()), 1);
    }

    #[test]
    fn second_commit_extends_the_history() {
        let (dir, repo) = init_repo();
        std::fs::write(dir.path().join("a.txt"), b"a").unwrap();
        commit_all(&repo, "a").unwrap();
        std::fs::write(dir.path().join("b.txt"), b"b").unwrap();
        commit_all(&repo, "b").unwrap();
        assert_eq!(count_commits(dir.path()), 2);
    }

    #[test]
    fn commit_uses_author_metadata() {
        let (dir, repo) = init_repo();
        std::fs::write(dir.path().join("x"), b"x").unwrap();
        commit_all(&repo, "msg").unwrap();
        let head = repo.head().unwrap().peel_to_commit().unwrap();
        assert_eq!(head.author().name(), Some("hearth"));
        assert_eq!(head.author().email(), Some("hearth@local"));
        assert_eq!(head.message().unwrap().trim(), "msg");
    }

    #[test]
    fn commit_picks_up_a_new_file() {
        let (dir, repo) = init_repo();
        std::fs::write(dir.path().join("first.txt"), b"a").unwrap();
        commit_all(&repo, "first").unwrap();
        std::fs::write(dir.path().join("second.txt"), b"b").unwrap();
        commit_all(&repo, "second").unwrap();
        let head = repo.head().unwrap().peel_to_tree().unwrap();
        assert!(head.get_path(Path::new("first.txt")).is_ok());
        assert!(head.get_path(Path::new("second.txt")).is_ok());
    }
}
