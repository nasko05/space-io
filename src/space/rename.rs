use age::secrecy::SecretString;

use crate::error::{AppError, AppResult};
use crate::space::git::commit_all;
use crate::space::meta::{self, MetaIndex};
use crate::space::paths::{resolve_under, with_age_suffix};
use crate::space::Space;

#[derive(Debug)]
pub struct MoveResult {
    pub path: String,
    pub is_directory: bool,
}

/// Rename or move a single file/folder. Thin wrapper around the bulk path
/// so there's exactly one code path for renames.
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

/// Apply a list of `(from, to)` renames inside a single git commit. The
/// previous single-rename path was double-committing (once for the meta
/// rewrite, once for the file move); this path writes the meta index
/// without committing, then issues exactly one commit covering both the
/// filesystem rename(s) and the meta update.
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
            if let Some(entry) = idx.paths.remove(&from) {
                idx.paths.insert(to.clone(), entry);
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
            let from_prefix = format!("{from}/");
            let to_prefix = format!("{to}/");
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

    let summary = if commit_lines.len() == 1 {
        format!("move: {}", commit_lines[0])
    } else {
        format!(
            "move ({} items):\n\n{}",
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
    use crate::space::test_helpers::{count_commits, make_space};

    #[test]
    fn renames_a_file() {
        let (d, s, p) = make_space("p");
        crate::space::write::write_file(&s, &p, "a.md", "x", None).unwrap();
        rename_path(&s, &p, "a.md", "b.md").unwrap();
        assert!(d.path().join("space/b.md.age").is_file());
        assert!(!d.path().join("space/a.md.age").exists());
    }

    #[test]
    fn move_into_subfolder_creates_intermediate_dirs() {
        let (d, s, p) = make_space("p");
        crate::space::write::write_file(&s, &p, "a.md", "x", None).unwrap();
        rename_path(&s, &p, "a.md", "Journal/2026/a.md").unwrap();
        assert!(d.path().join("space/Journal/2026/a.md.age").is_file());
    }

    #[test]
    fn rename_tags_follow_the_path() {
        let (_d, s, p) = make_space("p");
        crate::space::write::write_file(&s, &p, "a.md", "x", None).unwrap();
        crate::space::meta::set_tags(&s, &p, "a.md", vec!["t".into()]).unwrap();
        rename_path(&s, &p, "a.md", "b.md").unwrap();
        let idx = crate::space::meta::load(&s, &p).unwrap();
        assert!(!idx.paths.contains_key("a.md"));
        assert_eq!(idx.paths["b.md"].tags, vec!["t"]);
    }

    #[test]
    fn rename_to_existing_path_errors() {
        let (_d, s, p) = make_space("p");
        crate::space::write::write_file(&s, &p, "a.md", "x", None).unwrap();
        crate::space::write::write_file(&s, &p, "b.md", "y", None).unwrap();
        let err = rename_path(&s, &p, "a.md", "b.md").unwrap_err();
        assert!(matches!(err, AppError::BadRequest(_)));
    }

    #[test]
    fn rename_traversal_target_is_forbidden() {
        let (_d, s, p) = make_space("p");
        crate::space::write::write_file(&s, &p, "a.md", "x", None).unwrap();
        let err = rename_path(&s, &p, "a.md", "../escape.md").unwrap_err();
        assert!(matches!(err, AppError::Forbidden));
    }

    #[test]
    fn renames_a_folder_and_its_contents() {
        let (d, s, p) = make_space("p");
        crate::space::write::write_file(&s, &p, "Old/a.md", "x", None).unwrap();
        crate::space::write::write_file(&s, &p, "Old/sub/b.md", "y", None).unwrap();
        let r = rename_path(&s, &p, "Old", "New").unwrap();
        assert!(r.is_directory);
        assert!(d.path().join("space/New/a.md.age").is_file());
        assert!(d.path().join("space/New/sub/b.md.age").is_file());
        assert!(!d.path().join("space/Old").exists());
    }

    #[test]
    fn rename_missing_path_is_not_found() {
        let (_d, s, p) = make_space("p");
        let err = rename_path(&s, &p, "missing.md", "x.md").unwrap_err();
        assert!(matches!(err, AppError::NotFound));
    }

    #[test]
    fn single_rename_produces_a_single_commit() {
        let (d, s, p) = make_space("p");
        crate::space::write::write_file(&s, &p, "a.md", "x", None).unwrap();
        crate::space::meta::set_tags(&s, &p, "a.md", vec!["t".into()]).unwrap();
        let before = count_commits(&d.path().join("space"));
        rename_path(&s, &p, "a.md", "b.md").unwrap();
        let after = count_commits(&d.path().join("space"));
        assert_eq!(
            after - before,
            1,
            "rename should commit exactly once even with a meta rewrite",
        );
    }

    #[test]
    fn bulk_rename_produces_a_single_commit() {
        let (d, s, p) = make_space("p");
        crate::space::write::write_file(&s, &p, "a.md", "x", None).unwrap();
        crate::space::write::write_file(&s, &p, "b.md", "y", None).unwrap();
        crate::space::write::write_file(&s, &p, "c.md", "z", None).unwrap();
        let before = count_commits(&d.path().join("space"));
        rename_paths_bulk(
            &s,
            &p,
            vec![
                ("a.md".into(), "x.md".into()),
                ("b.md".into(), "y.md".into()),
                ("c.md".into(), "z.md".into()),
            ],
        )
        .unwrap();
        let after = count_commits(&d.path().join("space"));
        assert_eq!(
            after - before,
            1,
            "bulk rename should produce one commit, not N",
        );
        assert!(d.path().join("space/x.md.age").is_file());
        assert!(d.path().join("space/y.md.age").is_file());
        assert!(d.path().join("space/z.md.age").is_file());
    }
}
