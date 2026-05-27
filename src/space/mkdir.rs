use crate::error::{AppError, AppResult};
use crate::space::git::commit_all;
use crate::space::paths::resolve_under;
use crate::space::Space;

/// Create a folder under the space, with a tracked `.gitkeep` inside so the
/// folder survives the next commit (git ignores empty directories).
pub fn create_folder(space: &Space, path: &str) -> AppResult<()> {
    if path.is_empty() {
        return Err(AppError::BadRequest("empty folder path".into()));
    }
    let root = space.root();
    let resolved = resolve_under(&root, path)?;
    if resolved.exists() {
        return Err(AppError::BadRequest("folder already exists".into()));
    }
    std::fs::create_dir_all(&resolved)?;
    std::fs::write(resolved.join(".gitkeep"), b"")?;
    commit_all(&root, &format!("mkdir: {path}"))?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::space::test_helpers::make_space;

    #[test]
    fn creates_a_folder_with_gitkeep() {
        let (d, s, _) = make_space("p");
        create_folder(&s, "Notes/Tomorrow").unwrap();
        let f = d.path().join("space/Notes/Tomorrow");
        assert!(f.is_dir());
        assert!(f.join(".gitkeep").is_file());
    }

    #[test]
    fn rejects_empty_path() {
        let (_d, s, _) = make_space("p");
        assert!(matches!(
            create_folder(&s, "").unwrap_err(),
            AppError::BadRequest(_)
        ));
    }

    #[test]
    fn rejects_traversal() {
        let (_d, s, _) = make_space("p");
        assert!(matches!(
            create_folder(&s, "../escape").unwrap_err(),
            AppError::Forbidden
        ));
    }

    #[test]
    fn rejects_existing_folder() {
        let (_d, s, _) = make_space("p");
        create_folder(&s, "Once").unwrap();
        assert!(matches!(
            create_folder(&s, "Once").unwrap_err(),
            AppError::BadRequest(_)
        ));
    }
}
