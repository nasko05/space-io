use age::secrecy::SecretString;
use time::OffsetDateTime;

use crate::error::{AppError, AppResult};
use crate::space::git::commit_all;
use crate::space::meta;
use crate::space::paths::{resolve_under, with_age_suffix};
use crate::space::Space;

const TRASH_DIR: &str = ".trash";

#[derive(Debug)]
pub struct DeleteResult {
    pub trash_path: String,
}

/// Soft-delete: move the file or folder to `.trash/<timestamp>/<orig>/`.
/// Dotfile-prefixed folders are filtered out by `tree::build_tree` so the
/// trashed item disappears from the UI without losing data.
pub fn delete_to_trash(
    space: &Space,
    passphrase: &SecretString,
    path: &str,
) -> AppResult<DeleteResult> {
    if path.is_empty() {
        return Err(AppError::BadRequest("empty path".into()));
    }
    let root = space.root();
    let resolved = resolve_under(&root, path)?;
    let file = with_age_suffix(&resolved);

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
    let trash_rel = format!("{TRASH_DIR}/{ts}/{path}");
    let trash_resolved = root.join(&trash_rel);

    if file.is_file() {
        let dest = with_age_suffix(&trash_resolved);
        if let Some(parent) = dest.parent() {
            std::fs::create_dir_all(parent)?;
        }
        std::fs::rename(&file, &dest)?;
        meta::rewrite_paths(space, passphrase, path, &trash_rel, false)?;
        commit_all(&root, &format!("delete: {path}"))?;
        return Ok(DeleteResult {
            trash_path: trash_rel,
        });
    }

    if resolved.is_dir() {
        if let Some(parent) = trash_resolved.parent() {
            std::fs::create_dir_all(parent)?;
        }
        std::fs::rename(&resolved, &trash_resolved)?;
        meta::rewrite_paths(space, passphrase, path, &trash_rel, true)?;
        commit_all(&root, &format!("delete folder: {path}"))?;
        return Ok(DeleteResult {
            trash_path: trash_rel,
        });
    }

    Err(AppError::NotFound)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::space::test_helpers::make_space;

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
}
