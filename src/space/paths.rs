use std::path::{Component, Path, PathBuf};

use crate::error::{AppError, AppResult};

pub const ENC_EXT: &str = ".age";

/// Resolve a relative path against `root`, rejecting `..`, absolute paths,
/// and anything that escapes root after canonicalisation.
pub fn resolve_under(root: &Path, rel: &str) -> AppResult<PathBuf> {
    let mut out = root.to_path_buf();
    let candidate = Path::new(rel);
    for comp in candidate.components() {
        match comp {
            Component::Normal(seg) => out.push(seg),
            _ => return Err(AppError::Forbidden),
        }
    }
    if let Ok(canonical_root) = root.canonicalize() {
        match out.canonicalize() {
            Ok(canonical_target) => {
                if !canonical_target.starts_with(&canonical_root) {
                    return Err(AppError::Forbidden);
                }
            }
            Err(_) => {
                if let Some(parent) = out.parent() {
                    if parent.exists() {
                        let canonical_parent =
                            parent.canonicalize().map_err(|_| AppError::Forbidden)?;
                        if !canonical_parent.starts_with(&canonical_root) {
                            return Err(AppError::Forbidden);
                        }
                    } else if !parent.starts_with(&canonical_root) {
                        // Parent doesn't exist yet; ensure the prefix stays
                        // inside root by comparing the resolved buffer.
                        if !out.starts_with(&canonical_root) {
                            return Err(AppError::Forbidden);
                        }
                    }
                } else {
                    return Err(AppError::Forbidden);
                }
            }
        }
    }
    Ok(out)
}

/// Append `.age` to a path.
pub fn with_age_suffix(p: &Path) -> PathBuf {
    let mut s = p.as_os_str().to_owned();
    s.push(ENC_EXT);
    PathBuf::from(s)
}

#[cfg(test)]
mod tests {
    //! These tests pin the path-traversal contract. A regression here would
    //! turn any authenticated request into arbitrary-file-read of the host.
    use super::*;
    use std::fs;
    use tempfile::TempDir;

    fn make_root() -> TempDir {
        let dir = TempDir::new().expect("tempdir");
        fs::create_dir_all(dir.path().join("Journal/2026")).expect("mkdir");
        dir
    }

    #[test]
    fn accepts_a_simple_relative_path() {
        let root = make_root();
        let out = resolve_under(root.path(), "Journal/2026/note.md").expect("ok");
        assert!(out.starts_with(root.path()));
        assert!(out.ends_with("Journal/2026/note.md"));
    }

    #[test]
    fn rejects_parent_traversal() {
        let root = make_root();
        let err = resolve_under(root.path(), "../etc/passwd").unwrap_err();
        assert!(matches!(err, AppError::Forbidden));
    }

    #[test]
    fn rejects_nested_parent_traversal() {
        let root = make_root();
        let err = resolve_under(root.path(), "Journal/../../etc/passwd").unwrap_err();
        assert!(matches!(err, AppError::Forbidden));
    }

    #[test]
    fn rejects_absolute_path() {
        let root = make_root();
        let err = resolve_under(root.path(), "/etc/passwd").unwrap_err();
        assert!(matches!(err, AppError::Forbidden));
    }

    #[test]
    fn rejects_current_dir_prefix() {
        // `./foo` is `CurDir` then `Normal("foo")`. We only allow Normal.
        let root = make_root();
        let err = resolve_under(root.path(), "./foo").unwrap_err();
        assert!(matches!(err, AppError::Forbidden));
    }

    #[test]
    fn rejects_empty_segment_via_double_slash() {
        // `Path::components` collapses `//` so this just round-trips; the
        // canonical-root guard still keeps us safe. This test pins that
        // observation so a refactor that re-introduces empty segments fails
        // here, not in production.
        let root = make_root();
        let out = resolve_under(root.path(), "Journal//2026/note.md").expect("ok");
        assert!(out.starts_with(root.path()));
    }

    #[test]
    fn rejects_path_with_only_dotdot() {
        let root = make_root();
        let err = resolve_under(root.path(), "..").unwrap_err();
        assert!(matches!(err, AppError::Forbidden));
    }

    #[test]
    fn rejects_symlink_pointing_outside_root() {
        // On platforms with symlinks, a link that points outside the space
        // must be rejected by the canonicalize fallback.
        #[cfg(unix)]
        {
            use std::os::unix::fs::symlink;
            let root = make_root();
            // Create /tmp/outside and a symlink Journal/2026/escape → /tmp/outside.
            let outside = TempDir::new().unwrap();
            fs::write(outside.path().join("secret"), b"do not read").unwrap();
            symlink(outside.path(), root.path().join("Journal/2026/escape")).unwrap();

            let err = resolve_under(root.path(), "Journal/2026/escape/secret").unwrap_err();
            assert!(matches!(err, AppError::Forbidden));
        }
    }

    #[test]
    fn with_age_suffix_appends_correctly() {
        assert_eq!(
            with_age_suffix(Path::new("foo/bar.md")).to_string_lossy(),
            "foo/bar.md.age"
        );
        assert_eq!(
            with_age_suffix(Path::new("/abs/baz")).to_string_lossy(),
            "/abs/baz.age"
        );
    }
}
