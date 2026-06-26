use crate::error::{AppError, AppResult};
use crate::space::paths::{resolve_under, ENC_EXT};
use crate::space::Space;

#[derive(Debug)]
pub struct HistoryEntry {
    pub commit: String,
    pub message: String,
    pub author: String,
    pub when: String,
}

const MAX_HISTORY: usize = 50;

/// Walk the git log restricted to commits touching `<path>.age`.
///
/// Rejects path traversal up-front so the error model matches read/write/delete
/// (`Forbidden`, not an empty 200).
pub fn file_history(space: &Space, rel_path: &str) -> AppResult<Vec<HistoryEntry>> {
    resolve_under(&space.root(), rel_path)?;
    let target = format!("{rel_path}{ENC_EXT}");
    space.with_repo(|repo| {
        let mut walk = repo
            .revwalk()
            .map_err(|e| AppError::Internal(format!("git revwalk: {e}")))?;
        if walk.push_head().is_err() {
            return Ok(vec![]);
        }
        walk.set_sorting(git2::Sort::TIME)
            .map_err(|e| AppError::Internal(format!("git sort: {e}")))?;

        let mut out = Vec::new();
        for oid in walk.filter_map(Result::ok) {
            let Ok(commit) = repo.find_commit(oid) else {
                continue;
            };
            if !commit_touches(repo, &commit, &target).unwrap_or(false) {
                continue;
            }
            let when = format_git_time(commit.time());
            let author = commit.author().name().unwrap_or("unknown").to_string();
            out.push(HistoryEntry {
                commit: oid.to_string(),
                message: commit.message().unwrap_or("").trim().to_string(),
                author,
                when,
            });
            if out.len() >= MAX_HISTORY {
                break;
            }
        }
        Ok(out)
    })
}

fn commit_touches(
    repo: &git2::Repository,
    commit: &git2::Commit,
    target: &str,
) -> Result<bool, git2::Error> {
    let tree = commit.tree()?;
    if commit.parent_count() == 0 {
        return Ok(tree.get_path(std::path::Path::new(target)).is_ok());
    }
    let parent = commit.parent(0)?;
    let parent_tree = parent.tree()?;
    let diff = repo.diff_tree_to_tree(Some(&parent_tree), Some(&tree), None)?;
    let mut hit = false;
    diff.foreach(
        &mut |delta, _| {
            let path = delta
                .new_file()
                .path()
                .or_else(|| delta.old_file().path())
                .and_then(|p| p.to_str());
            if path == Some(target) {
                hit = true;
            }
            true
        },
        None,
        None,
        None,
    )?;
    Ok(hit)
}

fn format_git_time(t: git2::Time) -> String {
    let seconds = t.seconds();
    let dt = match time::OffsetDateTime::from_unix_timestamp(seconds) {
        Ok(d) => d,
        Err(_) => return String::new(),
    };
    dt.format(&time::format_description::well_known::Rfc3339)
        .unwrap_or_default()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::space::test_helpers::make_space;
    use crate::space::{create, write};

    #[test]
    fn empty_space_has_no_history() {
        let (_dir, space, _pass) = make_space("p");
        let history = file_history(&space, "missing.md").unwrap();
        assert!(history.is_empty());
    }

    #[test]
    fn returns_create_and_edit_for_one_file() {
        let (_dir, space, pass) = make_space("p");
        let created = create::create_file(&space, &pass, "F", Some("Note")).unwrap();
        write::write_file(&space, &pass, &created.path, "edited content", None).unwrap();

        let history = file_history(&space, &created.path).unwrap();
        assert_eq!(history.len(), 2);
        assert!(history[0].message.starts_with("Edit:"), "newest first");
        assert!(history[1].message.starts_with("Create:"));
    }

    #[test]
    fn does_not_include_commits_touching_other_paths() {
        let (_dir, space, pass) = make_space("p");
        write::write_file(&space, &pass, "a.md", "x", None).unwrap();
        write::write_file(&space, &pass, "b.md", "y", None).unwrap();

        let history = file_history(&space, "a.md").unwrap();
        assert_eq!(history.len(), 1);
        assert!(history[0].message.contains("a.md"));
        assert!(!history[0].message.contains("b.md"));
    }

    #[test]
    fn entries_have_author_and_rfc3339_time() {
        let (_dir, space, pass) = make_space("p");
        write::write_file(&space, &pass, "n.md", "x", None).unwrap();
        let history = file_history(&space, "n.md").unwrap();
        assert_eq!(history[0].author, "space-io");
        assert!(history[0].when.contains('T'));
        assert_eq!(history[0].commit.len(), 40, "git oid is 40 hex chars");
    }

    #[test]
    fn rejects_path_traversal() {
        let (_dir, space, _pass) = make_space("p");
        let err = file_history(&space, "../etc/passwd").unwrap_err();
        assert!(matches!(err, AppError::Forbidden));
    }
}
