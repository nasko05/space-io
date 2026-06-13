use crate::error::{AppError, AppResult};
use crate::space::git::commit_all;
use crate::space::paths::resolve_under;
use crate::space::Space;

/// Create a folder with a tracked `.gitkeep` inside so it survives commits (git
/// ignores empty directories).
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
    space.with_repo(|repo| commit_all(repo, &format!("mkdir: {path}")))?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::space::test_helpers::make_space;

    #[test]
    fn creates_a_folder_with_gitkeep() {
        let (dir, space, _) = make_space("p");
        create_folder(&space, "Notes/Tomorrow").unwrap();
        let folder = dir.path().join("space/Notes/Tomorrow");
        assert!(folder.is_dir());
        assert!(folder.join(".gitkeep").is_file());
    }

    #[test]
    fn rejects_empty_path() {
        let (_dir, space, _) = make_space("p");
        assert!(matches!(
            create_folder(&space, "").unwrap_err(),
            AppError::BadRequest(_)
        ));
    }

    #[test]
    fn rejects_traversal() {
        let (_dir, space, _) = make_space("p");
        assert!(matches!(
            create_folder(&space, "../escape").unwrap_err(),
            AppError::Forbidden
        ));
    }

    #[test]
    fn rejects_existing_folder() {
        let (_dir, space, _) = make_space("p");
        create_folder(&space, "Once").unwrap();
        assert!(matches!(
            create_folder(&space, "Once").unwrap_err(),
            AppError::BadRequest(_)
        ));
    }
}
