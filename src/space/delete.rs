use age::secrecy::SecretString;
use time::OffsetDateTime;

use crate::error::{AppError, AppResult};
use crate::space::git::commit_all;
use crate::space::meta::{self, MetaIndex};
use crate::space::paths::{resolve_under, with_age_suffix};
use crate::space::Space;

const TRASH_DIR: &str = ".trash";

#[derive(Debug)]
pub struct DeleteResult {
    pub trash_path: String,
}

/// Soft-delete a single file or folder. Thin wrapper around the bulk path
/// so there's exactly one delete code path.
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

/// Move a batch of files/folders to `.trash/<timestamp>/...` in a single
/// commit. All entries share the same timestamp directory so they're easy
/// to recover as a group. The previous path issued one commit per item,
/// which made multi-select delete crawl on encrypted vaults.
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
            if let Some(entry) = idx.paths.remove(&path) {
                idx.paths.insert(trash_rel.clone(), entry);
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
            let from_prefix = format!("{path}/");
            let to_prefix = format!("{trash_rel}/");
            let keys: Vec<String> = idx
                .paths
                .keys()
                .filter(|k| k.starts_with(&from_prefix))
                .cloned()
                .collect();
            for old_key in keys {
                let new_key = old_key.replacen(&from_prefix, &to_prefix, 1);
                if let Some(entry) = idx.paths.remove(&old_key) {
                    idx.paths.insert(new_key, entry);
                    meta_changed = true;
                }
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

    let summary = if commit_lines.len() == 1 {
        format!("delete: {}", commit_lines[0])
    } else {
        format!(
            "delete ({} items):\n\n{}",
            commit_lines.len(),
            commit_lines.join("\n")
        )
    };
    space.with_repo(|repo| commit_all(repo, &summary))?;

    Ok(results)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::space::test_helpers::make_space;

    fn count_commits(repo_path: &std::path::Path) -> usize {
        let repo = git2::Repository::open(repo_path).unwrap();
        let mut walk = repo.revwalk().unwrap();
        if walk.push_head().is_err() {
            return 0;
        }
        walk.filter_map(Result::ok).count()
    }

    #[test]
    fn deleting_a_file_moves_it_to_trash() {
        let (d, s, p) = make_space("p");
        crate::space::write::write_file(&s, &p, "a.md", "x", None).unwrap();
        let r = delete_to_trash(&s, &p, "a.md").unwrap();
        assert!(r.trash_path.starts_with(".trash/"));
        assert!(r.trash_path.ends_with("/a.md"));
        assert!(!d.path().join("space/a.md.age").exists());
        let trashed = d.path().join(format!("space/{}.age", r.trash_path));
        assert!(trashed.is_file(), "{trashed:?} should exist");
    }

    #[test]
    fn deleting_carries_tags_to_trash() {
        let (_d, s, p) = make_space("p");
        crate::space::write::write_file(&s, &p, "a.md", "x", None).unwrap();
        crate::space::meta::set_tags(&s, &p, "a.md", vec!["t".into()]).unwrap();
        let r = delete_to_trash(&s, &p, "a.md").unwrap();
        let idx = crate::space::meta::load(&s, &p).unwrap();
        assert!(!idx.paths.contains_key("a.md"));
        assert_eq!(idx.paths[&r.trash_path].tags, vec!["t"]);
    }

    #[test]
    fn deleting_a_folder_carries_subtree() {
        let (d, s, p) = make_space("p");
        crate::space::write::write_file(&s, &p, "F/a.md", "x", None).unwrap();
        crate::space::write::write_file(&s, &p, "F/sub/b.md", "y", None).unwrap();
        let r = delete_to_trash(&s, &p, "F").unwrap();
        assert!(!d.path().join("space/F").exists());
        let trash_root = d.path().join(format!("space/{}", r.trash_path));
        assert!(trash_root.join("a.md.age").is_file());
        assert!(trash_root.join("sub/b.md.age").is_file());
    }

    #[test]
    fn deleting_missing_path_is_not_found() {
        let (_d, s, p) = make_space("p");
        let err = delete_to_trash(&s, &p, "missing.md").unwrap_err();
        assert!(matches!(err, AppError::NotFound));
    }

    #[test]
    fn deleting_empty_path_is_bad_request() {
        let (_d, s, p) = make_space("p");
        let err = delete_to_trash(&s, &p, "").unwrap_err();
        assert!(matches!(err, AppError::BadRequest(_)));
    }

    #[test]
    fn single_delete_produces_a_single_commit() {
        let (d, s, p) = make_space("p");
        crate::space::write::write_file(&s, &p, "a.md", "x", None).unwrap();
        crate::space::meta::set_tags(&s, &p, "a.md", vec!["t".into()]).unwrap();
        let before = count_commits(&d.path().join("space"));
        delete_to_trash(&s, &p, "a.md").unwrap();
        let after = count_commits(&d.path().join("space"));
        assert_eq!(
            after - before,
            1,
            "delete should commit exactly once even with a meta rewrite",
        );
    }

    #[test]
    fn bulk_delete_produces_a_single_commit() {
        let (d, s, p) = make_space("p");
        crate::space::write::write_file(&s, &p, "a.md", "x", None).unwrap();
        crate::space::write::write_file(&s, &p, "b.md", "y", None).unwrap();
        crate::space::write::write_file(&s, &p, "c.md", "z", None).unwrap();
        let before = count_commits(&d.path().join("space"));
        let results =
            delete_to_trash_bulk(&s, &p, vec!["a.md".into(), "b.md".into(), "c.md".into()])
                .unwrap();
        let after = count_commits(&d.path().join("space"));
        assert_eq!(
            after - before,
            1,
            "bulk delete should produce one commit, not N",
        );
        assert_eq!(results.len(), 3);
        // All three files land under the same timestamp directory.
        let parent = std::path::Path::new(&results[0].trash_path)
            .parent()
            .unwrap();
        for r in &results {
            assert_eq!(
                std::path::Path::new(&r.trash_path).parent().unwrap(),
                parent
            );
        }
    }
}
