use crate::error::{AppError, AppResult};
use crate::space::paths::ENC_EXT;
use crate::space::Space;

pub struct HistoryEntry {
    pub commit: String,
    pub message: String,
    pub author: String,
    pub when: String,
}

const MAX_HISTORY: usize = 50;

/// Walk the git log restricted to commits touching `<path>.age`.
pub fn file_history(space: &Space, rel_path: &str) -> AppResult<Vec<HistoryEntry>> {
    let repo = git2::Repository::open(space.root())
        .map_err(|e| AppError::Internal(format!("git open: {e}")))?;
    let mut walk = repo
        .revwalk()
        .map_err(|e| AppError::Internal(format!("git revwalk: {e}")))?;
    if walk.push_head().is_err() {
        return Ok(vec![]);
    }
    walk.set_sorting(git2::Sort::TIME)
        .map_err(|e| AppError::Internal(format!("git sort: {e}")))?;

    let target = format!("{rel_path}{ENC_EXT}");
    let mut out = Vec::new();
    for oid in walk.filter_map(Result::ok) {
        let Ok(commit) = repo.find_commit(oid) else {
            continue;
        };
        if !commit_touches(&repo, &commit, &target).unwrap_or(false) {
            continue;
        }
        let when = format_git_time(commit.time());
        let author = commit
            .author()
            .name()
            .unwrap_or("unknown")
            .to_string();
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
}

fn commit_touches(
    repo: &git2::Repository,
    commit: &git2::Commit,
    target: &str,
) -> Result<bool, git2::Error> {
    let tree = commit.tree()?;
    if commit.parent_count() == 0 {
        // Root commit: file present in tree counts.
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
