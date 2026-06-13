use age::secrecy::SecretString;

use crate::error::{AppError, AppResult};
use crate::space::git::{batch_commit_message, commit_all};
use crate::space::meta::{self, MetaIndex};
use crate::space::paths::{resolve_under, with_age_suffix};
use crate::space::Space;

#[derive(Debug)]
pub struct MoveResult {
    pub path: String,
    pub is_directory: bool,
}

/// Rename or move a single file/folder; a thin wrapper so there is one rename
/// code path.
pub fn rename_path(
    space: &Space,
    passphrase: &SecretString,
    from: &str,
    to: &str,
) -> AppResult<MoveResult> {
    let mut results =
        rename_paths_bulk(space, passphrase, vec![(from.to_string(), to.to_string())])?;
    results
        .pop()
        .ok_or_else(|| AppError::Internal("rename produced no result".into()))
}

/// Apply a list of `(from, to)` renames in a single git commit covering both the
/// filesystem moves and the meta-index update.
pub fn rename_paths_bulk(
    space: &Space,
    passphrase: &SecretString,
    pairs: Vec<(String, String)>,
) -> AppResult<Vec<MoveResult>> {
    if pairs.is_empty() {
        return Ok(vec![]);
    }
    let root = space.root();
    let cached = meta::load(space, passphrase)?;
    let mut idx: MetaIndex = (*cached).clone();
    let mut meta_changed = false;
    let mut results: Vec<MoveResult> = Vec::with_capacity(pairs.len());
    let mut commit_lines: Vec<String> = Vec::with_capacity(pairs.len());
    let mut clear_full_cache = false;

    for (from, to) in pairs {
        if from == to {
            return Err(AppError::BadRequest("source and destination match".into()));
        }
        let from_resolved = resolve_under(&root, &from)?;
        let to_resolved = resolve_under(&root, &to)?;
        let from_file = with_age_suffix(&from_resolved);
        let to_file = with_age_suffix(&to_resolved);

        if from_file.is_file() {
            if to_file.exists() {
                return Err(AppError::BadRequest("destination exists".into()));
            }
            if let Some(parent) = to_file.parent() {
                std::fs::create_dir_all(parent)?;
            }
            std::fs::rename(&from_file, &to_file)?;
            space.cache().invalidate(&from_file.to_string_lossy());
            space.cache().invalidate(&to_file.to_string_lossy());
            if idx.move_entry(&from, &to) {
                meta_changed = true;
            }
            commit_lines.push(format!("{from} → {to}"));
            results.push(MoveResult {
                path: to,
                is_directory: false,
            });
        } else if from_resolved.is_dir() {
            if to_resolved.exists() {
                return Err(AppError::BadRequest("destination exists".into()));
            }
            if let Some(parent) = to_resolved.parent() {
                std::fs::create_dir_all(parent)?;
            }
            std::fs::rename(&from_resolved, &to_resolved)?;
            clear_full_cache = true;
            if idx.move_subtree(&from, &to) {
                meta_changed = true;
            }
            commit_lines.push(format!("{from}/ → {to}/"));
            results.push(MoveResult {
                path: to,
                is_directory: true,
            });
        } else {
            return Err(AppError::NotFound);
        }
    }

    if clear_full_cache {
        space.cache().clear();
    }
    if meta_changed {
        meta::write_index(space, passphrase, &idx)?;
    }

    let summary = batch_commit_message("move", &commit_lines);
    space.with_repo(|repo| commit_all(repo, &summary))?;

    Ok(results)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::space::test_helpers::{count_commits, make_space};
    use crate::space::{meta, write};

    #[test]
    fn renames_a_file() {
        let (dir, space, pass) = make_space("p");
        write::write_file(&space, &pass, "a.md", "x", None).unwrap();
        rename_path(&space, &pass, "a.md", "b.md").unwrap();
        assert!(dir.path().join("space/b.md.age").is_file());
        assert!(!dir.path().join("space/a.md.age").exists());
    }

    #[test]
    fn move_into_subfolder_creates_intermediate_dirs() {
        let (dir, space, pass) = make_space("p");
        write::write_file(&space, &pass, "a.md", "x", None).unwrap();
        rename_path(&space, &pass, "a.md", "Journal/2026/a.md").unwrap();
        assert!(dir.path().join("space/Journal/2026/a.md.age").is_file());
    }

    #[test]
    fn rename_tags_follow_the_path() {
        let (_dir, space, pass) = make_space("p");
        write::write_file(&space, &pass, "a.md", "x", None).unwrap();
        meta::set_tags(&space, &pass, "a.md", vec!["t".into()]).unwrap();
        rename_path(&space, &pass, "a.md", "b.md").unwrap();
        let idx = meta::load(&space, &pass).unwrap();
        assert!(!idx.paths.contains_key("a.md"));
        assert_eq!(idx.paths["b.md"].tags, vec!["t"]);
    }

    #[test]
    fn rename_to_existing_path_errors() {
        let (_dir, space, pass) = make_space("p");
        write::write_file(&space, &pass, "a.md", "x", None).unwrap();
        write::write_file(&space, &pass, "b.md", "y", None).unwrap();
        let err = rename_path(&space, &pass, "a.md", "b.md").unwrap_err();
        assert!(matches!(err, AppError::BadRequest(_)));
    }

    #[test]
    fn rename_traversal_target_is_forbidden() {
        let (_dir, space, pass) = make_space("p");
        write::write_file(&space, &pass, "a.md", "x", None).unwrap();
        let err = rename_path(&space, &pass, "a.md", "../escape.md").unwrap_err();
        assert!(matches!(err, AppError::Forbidden));
    }

    #[test]
    fn renames_a_folder_and_its_contents() {
        let (dir, space, pass) = make_space("p");
        write::write_file(&space, &pass, "Old/a.md", "x", None).unwrap();
        write::write_file(&space, &pass, "Old/sub/b.md", "y", None).unwrap();
        let result = rename_path(&space, &pass, "Old", "New").unwrap();
        assert!(result.is_directory);
        assert!(dir.path().join("space/New/a.md.age").is_file());
        assert!(dir.path().join("space/New/sub/b.md.age").is_file());
        assert!(!dir.path().join("space/Old").exists());
    }

    #[test]
    fn rename_missing_path_is_not_found() {
        let (_dir, space, pass) = make_space("p");
        let err = rename_path(&space, &pass, "missing.md", "x.md").unwrap_err();
        assert!(matches!(err, AppError::NotFound));
    }

    #[test]
    fn single_rename_produces_a_single_commit() {
        let (dir, space, pass) = make_space("p");
        write::write_file(&space, &pass, "a.md", "x", None).unwrap();
        meta::set_tags(&space, &pass, "a.md", vec!["t".into()]).unwrap();
        let before = count_commits(&dir.path().join("space"));
        rename_path(&space, &pass, "a.md", "b.md").unwrap();
        let after = count_commits(&dir.path().join("space"));
        assert_eq!(
            after - before,
            1,
            "rename should commit exactly once even with a meta rewrite",
        );
    }

    #[test]
    fn bulk_rename_produces_a_single_commit() {
        let (dir, space, pass) = make_space("p");
        write::write_file(&space, &pass, "a.md", "x", None).unwrap();
        write::write_file(&space, &pass, "b.md", "y", None).unwrap();
        write::write_file(&space, &pass, "c.md", "z", None).unwrap();
        let before = count_commits(&dir.path().join("space"));
        rename_paths_bulk(
            &space,
            &pass,
            vec![
                ("a.md".into(), "x.md".into()),
                ("b.md".into(), "y.md".into()),
                ("c.md".into(), "z.md".into()),
            ],
        )
        .unwrap();
        let after = count_commits(&dir.path().join("space"));
        assert_eq!(
            after - before,
            1,
            "bulk rename should produce one commit, not N"
        );
        assert!(dir.path().join("space/x.md.age").is_file());
        assert!(dir.path().join("space/y.md.age").is_file());
        assert!(dir.path().join("space/z.md.age").is_file());
    }
}
