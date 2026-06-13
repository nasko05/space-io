use age::secrecy::SecretString;
use time::OffsetDateTime;

use crate::error::{AppError, AppResult};
use crate::space::git::{batch_commit_message, commit_all};
use crate::space::meta::{self, MetaIndex};
use crate::space::paths::{resolve_under, with_age_suffix};
use crate::space::Space;

const TRASH_DIR: &str = ".trash";

#[derive(Debug)]
pub struct DeleteResult {
    pub trash_path: String,
}

/// Soft-delete a single file or folder; a thin wrapper so there is one delete
/// code path.
pub fn delete_to_trash(
    space: &Space,
    passphrase: &SecretString,
    path: &str,
) -> AppResult<DeleteResult> {
    let mut results = delete_to_trash_bulk(space, passphrase, vec![path.to_string()])?;
    results
        .pop()
        .ok_or_else(|| AppError::Internal("delete produced no result".into()))
}

/// Move a batch of files/folders to `.trash/<timestamp>/...` in a single commit.
/// All entries share one timestamp directory so they recover as a group.
pub fn delete_to_trash_bulk(
    space: &Space,
    passphrase: &SecretString,
    paths: Vec<String>,
) -> AppResult<Vec<DeleteResult>> {
    if paths.is_empty() {
        return Ok(vec![]);
    }
    let root = space.root();
    let now = OffsetDateTime::now_utc();
    let ts = format!(
        "{:04}{:02}{:02}-{:02}{:02}{:02}",
        now.year(),
        u8::from(now.month()),
        now.day(),
        now.hour(),
        now.minute(),
        now.second(),
    );

    let cached = meta::load(space, passphrase)?;
    let mut idx: MetaIndex = (*cached).clone();
    let mut meta_changed = false;
    let mut results: Vec<DeleteResult> = Vec::with_capacity(paths.len());
    let mut commit_lines: Vec<String> = Vec::with_capacity(paths.len());
    let mut clear_full_cache = false;

    for path in paths {
        if path.is_empty() {
            return Err(AppError::BadRequest("empty path".into()));
        }
        let resolved = resolve_under(&root, &path)?;
        let file = with_age_suffix(&resolved);
        let trash_rel = format!("{TRASH_DIR}/{ts}/{path}");
        let trash_resolved = root.join(&trash_rel);

        if file.is_file() {
            let dest = with_age_suffix(&trash_resolved);
            if let Some(parent) = dest.parent() {
                std::fs::create_dir_all(parent)?;
            }
            std::fs::rename(&file, &dest)?;
            space.cache().invalidate(&file.to_string_lossy());
            if idx.move_entry(&path, &trash_rel) {
                meta_changed = true;
            }
            commit_lines.push(path.clone());
            results.push(DeleteResult {
                trash_path: trash_rel,
            });
        } else if resolved.is_dir() {
            if let Some(parent) = trash_resolved.parent() {
                std::fs::create_dir_all(parent)?;
            }
            std::fs::rename(&resolved, &trash_resolved)?;
            clear_full_cache = true;
            if idx.move_subtree(&path, &trash_rel) {
                meta_changed = true;
            }
            commit_lines.push(format!("{path}/"));
            results.push(DeleteResult {
                trash_path: trash_rel,
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

    let summary = batch_commit_message("delete", &commit_lines);
    space.with_repo(|repo| commit_all(repo, &summary))?;

    Ok(results)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::space::test_helpers::{count_commits, make_space};
    use crate::space::{meta, write};

    #[test]
    fn deleting_a_file_moves_it_to_trash() {
        let (dir, space, pass) = make_space("p");
        write::write_file(&space, &pass, "a.md", "x", None).unwrap();
        let result = delete_to_trash(&space, &pass, "a.md").unwrap();
        assert!(result.trash_path.starts_with(".trash/"));
        assert!(result.trash_path.ends_with("/a.md"));
        assert!(!dir.path().join("space/a.md.age").exists());
        let trashed = dir.path().join(format!("space/{}.age", result.trash_path));
        assert!(trashed.is_file(), "{trashed:?} should exist");
    }

    #[test]
    fn deleting_carries_tags_to_trash() {
        let (_dir, space, pass) = make_space("p");
        write::write_file(&space, &pass, "a.md", "x", None).unwrap();
        meta::set_tags(&space, &pass, "a.md", vec!["t".into()]).unwrap();
        let result = delete_to_trash(&space, &pass, "a.md").unwrap();
        let idx = meta::load(&space, &pass).unwrap();
        assert!(!idx.paths.contains_key("a.md"));
        assert_eq!(idx.paths[&result.trash_path].tags, vec!["t"]);
    }

    #[test]
    fn deleting_a_folder_carries_subtree() {
        let (dir, space, pass) = make_space("p");
        write::write_file(&space, &pass, "F/a.md", "x", None).unwrap();
        write::write_file(&space, &pass, "F/sub/b.md", "y", None).unwrap();
        let result = delete_to_trash(&space, &pass, "F").unwrap();
        assert!(!dir.path().join("space/F").exists());
        let trash_root = dir.path().join(format!("space/{}", result.trash_path));
        assert!(trash_root.join("a.md.age").is_file());
        assert!(trash_root.join("sub/b.md.age").is_file());
    }

    #[test]
    fn deleting_missing_path_is_not_found() {
        let (_dir, space, pass) = make_space("p");
        let err = delete_to_trash(&space, &pass, "missing.md").unwrap_err();
        assert!(matches!(err, AppError::NotFound));
    }

    #[test]
    fn deleting_empty_path_is_bad_request() {
        let (_dir, space, pass) = make_space("p");
        let err = delete_to_trash(&space, &pass, "").unwrap_err();
        assert!(matches!(err, AppError::BadRequest(_)));
    }

    #[test]
    fn single_delete_produces_a_single_commit() {
        let (dir, space, pass) = make_space("p");
        write::write_file(&space, &pass, "a.md", "x", None).unwrap();
        meta::set_tags(&space, &pass, "a.md", vec!["t".into()]).unwrap();
        let before = count_commits(&dir.path().join("space"));
        delete_to_trash(&space, &pass, "a.md").unwrap();
        let after = count_commits(&dir.path().join("space"));
        assert_eq!(
            after - before,
            1,
            "delete should commit exactly once even with a meta rewrite",
        );
    }

    #[test]
    fn bulk_delete_produces_a_single_commit() {
        let (dir, space, pass) = make_space("p");
        write::write_file(&space, &pass, "a.md", "x", None).unwrap();
        write::write_file(&space, &pass, "b.md", "y", None).unwrap();
        write::write_file(&space, &pass, "c.md", "z", None).unwrap();
        let before = count_commits(&dir.path().join("space"));
        let results = delete_to_trash_bulk(
            &space,
            &pass,
            vec!["a.md".into(), "b.md".into(), "c.md".into()],
        )
        .unwrap();
        let after = count_commits(&dir.path().join("space"));
        assert_eq!(
            after - before,
            1,
            "bulk delete should produce one commit, not N"
        );
        assert_eq!(results.len(), 3);
        let parent = std::path::Path::new(&results[0].trash_path)
            .parent()
            .unwrap();
        for result in &results {
            assert_eq!(
                std::path::Path::new(&result.trash_path).parent().unwrap(),
                parent
            );
        }
    }
}
