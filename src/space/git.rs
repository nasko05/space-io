use std::path::Path;

use git2::Repository;

use crate::error::{AppError, AppResult};

/// Stage every change under `repo` and commit. For operations that touch many
/// files (init, bulk move/delete) or can't enumerate what changed. Prefer
/// [`commit_paths`] when the caller knows the touched paths: `add_all(["*"])`
/// walks the whole working tree and dominates wall-clock time on large vaults.
pub fn commit_all(repo: &Repository, message: &str) -> AppResult<()> {
    commit_with(repo, message, |index| {
        index
            .add_all(["*"].iter(), git2::IndexAddOption::DEFAULT, None)
            .map_err(|e| AppError::Internal(format!("git add: {e}")))
    })
}

/// Stage only the listed working-tree paths (relative to the repo root) and
/// commit. Each path can refer to a file that's been created, modified, or
/// deleted; `add_path` plus `update_all` covers all three so callers don't
/// have to know which case they're in.
pub fn commit_paths<I, S>(repo: &Repository, message: &str, paths: I) -> AppResult<()>
where
    I: IntoIterator<Item = S>,
    S: AsRef<Path>,
{
    let paths: Vec<std::path::PathBuf> = paths
        .into_iter()
        .map(|p| p.as_ref().to_path_buf())
        .collect();
    commit_with(repo, message, |index| {
        // `update_all` catches modifications + deletions of tracked paths;
        // `add_path` then catches brand-new ones — together mirroring
        // `add --all <path>`.
        let path_specs: Vec<&Path> = paths.iter().map(|p| p.as_path()).collect();
        index
            .update_all(&path_specs, None)
            .map_err(|e| AppError::Internal(format!("git update_all: {e}")))?;
        for path in &paths {
            if repo
                .workdir()
                .is_some_and(|workdir| workdir.join(path).exists())
            {
                index
                    .add_path(path)
                    .map_err(|e| AppError::Internal(format!("git add_path: {e}")))?;
            }
        }
        Ok(())
    })
}

fn commit_with<F>(repo: &Repository, message: &str, stage: F) -> AppResult<()>
where
    F: FnOnce(&mut git2::Index) -> AppResult<()>,
{
    let mut index = repo
        .index()
        .map_err(|e| AppError::Internal(format!("git index: {e}")))?;
    stage(&mut index)?;
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
    // HEAD → commit is our parent; a missing HEAD means the root commit (no
    // parents). A HEAD that won't peel is a real failure and must bubble up —
    // treating it as "no parents" would orphan history.
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

/// Commit message for a batch operation: `"<verb>: <line>"` for one item, or a
/// `"<verb> (N items):"` header followed by each line.
pub fn batch_commit_message(verb: &str, lines: &[String]) -> String {
    if lines.len() == 1 {
        format!("{verb}: {}", lines[0])
    } else {
        format!("{verb} ({} items):\n\n{}", lines.len(), lines.join("\n"))
    }
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

    #[test]
    fn commit_paths_stages_only_listed_paths() {
        let (dir, repo) = init_repo();
        std::fs::write(dir.path().join("a.txt"), b"a").unwrap();
        std::fs::write(dir.path().join("b.txt"), b"b").unwrap();
        commit_paths(&repo, "only a", [Path::new("a.txt")]).unwrap();
        let head = repo.head().unwrap().peel_to_tree().unwrap();
        assert!(head.get_path(Path::new("a.txt")).is_ok());
        assert!(
            head.get_path(Path::new("b.txt")).is_err(),
            "b.txt should NOT have been staged",
        );
    }

    #[test]
    fn commit_paths_picks_up_modification() {
        let (dir, repo) = init_repo();
        std::fs::write(dir.path().join("a.txt"), b"v1").unwrap();
        commit_paths(&repo, "v1", [Path::new("a.txt")]).unwrap();
        std::fs::write(dir.path().join("a.txt"), b"v2").unwrap();
        commit_paths(&repo, "v2", [Path::new("a.txt")]).unwrap();
        let head = repo.head().unwrap().peel_to_commit().unwrap();
        let blob_oid = head
            .tree()
            .unwrap()
            .get_path(Path::new("a.txt"))
            .unwrap()
            .id();
        let blob = repo.find_blob(blob_oid).unwrap();
        assert_eq!(blob.content(), b"v2");
    }

    #[test]
    fn commit_paths_picks_up_deletion() {
        let (dir, repo) = init_repo();
        std::fs::write(dir.path().join("a.txt"), b"a").unwrap();
        commit_paths(&repo, "add", [Path::new("a.txt")]).unwrap();
        std::fs::remove_file(dir.path().join("a.txt")).unwrap();
        commit_paths(&repo, "remove", [Path::new("a.txt")]).unwrap();
        let head = repo.head().unwrap().peel_to_tree().unwrap();
        assert!(
            head.get_path(Path::new("a.txt")).is_err(),
            "a.txt should be gone after commit_paths sees the deletion",
        );
    }
}
