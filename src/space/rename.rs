use age::secrecy::SecretString;

use crate::error::{AppError, AppResult};
use crate::space::git::commit_all;
use crate::space::meta;
use crate::space::paths::{resolve_under, with_age_suffix};
use crate::space::Space;

#[derive(Debug)]
pub struct MoveResult {
    pub path: String,
    pub is_directory: bool,
}

/// Rename or move a file/folder. The first existing match wins:
///
///   1. A file at `<from>.age`
///   2. A directory at `<from>`
///
/// The metadata index is rewritten so tags follow the path.
pub fn rename_path(
    space: &Space,
    passphrase: &SecretString,
    from: &str,
    to: &str,
) -> AppResult<MoveResult> {
    if from == to {
        return Err(AppError::BadRequest("source and destination match".into()));
    }
    let root = space.root();
    let from_resolved = resolve_under(&root, from)?;
    let to_resolved = resolve_under(&root, to)?;

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
        meta::rewrite_paths(space, passphrase, from, to, false)?;
        space.with_repo(|repo| commit_all(repo, &format!("move: {from} → {to}")))?;
        return Ok(MoveResult {
            path: to.to_string(),
            is_directory: false,
        });
    }

    if from_resolved.is_dir() {
        if to_resolved.exists() {
            return Err(AppError::BadRequest("destination exists".into()));
        }
        if let Some(parent) = to_resolved.parent() {
            std::fs::create_dir_all(parent)?;
        }
        std::fs::rename(&from_resolved, &to_resolved)?;
        space.cache().clear();
        meta::rewrite_paths(space, passphrase, from, to, true)?;
        space.with_repo(|repo| commit_all(repo, &format!("move folder: {from} → {to}")))?;
        return Ok(MoveResult {
            path: to.to_string(),
            is_directory: true,
        });
    }

    Err(AppError::NotFound)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::space::test_helpers::make_space;

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
}
