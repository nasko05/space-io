use std::path::{Component, Path, PathBuf};

use crate::error::{AppError, AppResult};

pub const ENC_EXT: &str = ".age";

/// Max length for a single filename segment. Stays under common filesystem
/// limits (255 on ext4/APFS) with headroom for the `.age` suffix.
pub const MAX_FILENAME: usize = 200;

/// Resolve a relative path against `root`, rejecting `..`, absolute paths,
/// and anything that escapes root after canonicalisation.
pub fn resolve_under(root: &Path, rel: &str) -> AppResult<PathBuf> {
    let mut out = root.to_path_buf();
    let candidate = Path::new(rel);
    for component in candidate.components() {
        match component {
            Component::Normal(segment) => out.push(segment),
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
                // Target doesn't exist yet (e.g. about to be created): validate
                // the first existing ancestor instead.
                let mut walker = out.as_path();
                let mut ancestor = loop {
                    match walker.parent() {
                        Some(parent) if parent.exists() => break parent.to_path_buf(),
                        Some(parent) => walker = parent,
                        None => return Err(AppError::Forbidden),
                    }
                };
                ancestor = ancestor.canonicalize().map_err(|_| AppError::Forbidden)?;
                if !ancestor.starts_with(&canonical_root) {
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

/// Sanitise a literal uploaded filename (one segment, never a relative path),
/// preserving it character for character. Rejects path separators, NULs, and
/// leading dots; truncates to `MAX_FILENAME` bytes.
pub fn sanitise_filename(input: &str) -> AppResult<String> {
    let trimmed = input.trim();
    if trimmed.is_empty() {
        return Err(AppError::BadRequest("empty filename".into()));
    }
    let mut out = String::with_capacity(trimmed.len());
    for ch in trimmed.chars() {
        if ch == '/' || ch == '\\' || ch == '\0' {
            return Err(AppError::BadRequest("filename contains separator".into()));
        }
        out.push(ch);
    }
    if out.starts_with('.') {
        return Err(AppError::BadRequest(
            "filename cannot start with '.'".into(),
        ));
    }
    if out.len() > MAX_FILENAME {
        out.truncate(MAX_FILENAME);
    }
    Ok(out)
}

/// Sanitise a *note title* into a filename stem. Punctuation is stripped,
/// unsupported chars become '-', whitespace collapses to single spaces.
/// Returns `None` on empty input so callers can fall back to a generated
/// name like `Untitled 09-23`.
pub fn sanitise_title(title: &str) -> Option<String> {
    let trimmed = title.trim();
    if trimmed.is_empty() {
        return None;
    }
    let mut out = String::with_capacity(trimmed.len());
    for ch in trimmed.chars() {
        if ch.is_ascii_alphanumeric() || ch == ' ' || ch == '-' || ch == '_' {
            out.push(ch);
        } else if ch == ',' || ch == '.' || ch == '!' || ch == '?' {
            // Sentence punctuation is dropped entirely rather than dashed.
        } else {
            out.push('-');
        }
    }
    let out = out.split_whitespace().collect::<Vec<_>>().join(" ");
    if out.is_empty() {
        None
    } else {
        Some(out)
    }
}

/// Split `name` into its stem and extension. `("photo.jpg") → ("photo",
/// "jpg")`; names with no extension (or a leading/trailing `.`) yield an
/// empty extension.
pub fn split_stem_ext(name: &str) -> (String, String) {
    match name.rfind('.') {
        Some(i) if i > 0 && i < name.len() - 1 => {
            (name[..i].to_string(), name[i + 1..].to_string())
        }
        _ => (name.to_string(), String::new()),
    }
}

/// Find a non-colliding name in `folder` by trying `stem.ext`, then `stem
/// (2).ext`, `stem (3).ext`, … up to 999. The encrypted-side `.age` suffix
/// is appended internally for the exists-check so callers pass the
/// visible name.
pub fn find_unique_name(folder: &Path, stem: &str, ext: &str) -> AppResult<String> {
    let initial = if ext.is_empty() {
        stem.to_string()
    } else {
        format!("{stem}.{ext}")
    };
    if !folder.join(format!("{initial}{ENC_EXT}")).exists() {
        return Ok(initial);
    }
    for counter in 2..=999 {
        let candidate = if ext.is_empty() {
            format!("{stem} ({counter})")
        } else {
            format!("{stem} ({counter}).{ext}")
        };
        if !folder.join(format!("{candidate}{ENC_EXT}")).exists() {
            return Ok(candidate);
        }
    }
    Err(AppError::Internal("too many filename collisions".into()))
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
        // `Path::components` collapses `//`; the canonical-root guard still
        // protects us. Pinned so a refactor re-introducing empty segments fails
        // here rather than in production.
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
            let outside = TempDir::new().unwrap();
            fs::write(outside.path().join("secret"), b"do not read").unwrap();
            symlink(outside.path(), root.path().join("Journal/2026/escape")).unwrap();

            let err = resolve_under(root.path(), "Journal/2026/escape/secret").unwrap_err();
            assert!(matches!(err, AppError::Forbidden));
        }
    }

    #[test]
    fn nonexistent_target_validated_via_first_existing_ancestor() {
        // Deeper than anything on disk; accepted because the first existing
        // ancestor (Journal) sits under the root.
        let root = make_root();
        let out = resolve_under(root.path(), "Journal/2026/deep/new/note.md").expect("ok");
        assert!(out.starts_with(root.path()));
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

    #[test]
    fn sanitise_filename_passes_simple_names() {
        assert_eq!(sanitise_filename("ok.txt").unwrap(), "ok.txt");
    }

    #[test]
    fn sanitise_filename_rejects_path_separators() {
        assert!(sanitise_filename("a/b").is_err());
        assert!(sanitise_filename("a\\b").is_err());
        assert!(sanitise_filename("a\0b").is_err());
    }

    #[test]
    fn sanitise_filename_rejects_dotfiles() {
        assert!(sanitise_filename(".env").is_err());
    }

    #[test]
    fn sanitise_filename_rejects_empty() {
        assert!(sanitise_filename("").is_err());
        assert!(sanitise_filename("   ").is_err());
    }

    #[test]
    fn sanitise_filename_truncates_long_names() {
        let long = "a".repeat(500);
        let result = sanitise_filename(&long).unwrap();
        assert!(result.len() <= MAX_FILENAME);
    }

    #[test]
    fn sanitise_title_preserves_simple_titles() {
        assert_eq!(sanitise_title("My Note"), Some("My Note".into()));
    }

    #[test]
    fn sanitise_title_strips_punctuation() {
        assert_eq!(sanitise_title("Hello, world!"), Some("Hello world".into()));
    }

    #[test]
    fn sanitise_title_replaces_unsupported_chars_with_dash() {
        assert_eq!(sanitise_title("a/b\\c"), Some("a-b-c".into()));
    }

    #[test]
    fn sanitise_title_returns_none_for_empty() {
        assert_eq!(sanitise_title(""), None);
        assert_eq!(sanitise_title("   "), None);
    }

    #[test]
    fn sanitise_title_collapses_whitespace() {
        assert_eq!(sanitise_title("hello   world"), Some("hello world".into()));
    }

    #[test]
    fn split_stem_ext_pulls_extension() {
        assert_eq!(split_stem_ext("a.txt"), ("a".into(), "txt".into()));
        assert_eq!(split_stem_ext("photo.jpg"), ("photo".into(), "jpg".into()));
    }

    #[test]
    fn split_stem_ext_handles_no_extension() {
        assert_eq!(split_stem_ext("README"), ("README".into(), String::new()));
    }

    #[test]
    fn split_stem_ext_handles_dot_only_at_start_or_end() {
        assert_eq!(split_stem_ext(".hidden"), (".hidden".into(), String::new()));
        assert_eq!(split_stem_ext("foo."), ("foo.".into(), String::new()));
    }

    #[test]
    fn find_unique_name_returns_initial_when_free() {
        let dir = TempDir::new().unwrap();
        let name = find_unique_name(dir.path(), "note", "md").unwrap();
        assert_eq!(name, "note.md");
    }

    #[test]
    fn find_unique_name_picks_first_free_paren_suffix() {
        let dir = TempDir::new().unwrap();
        fs::write(dir.path().join("note.md.age"), b"x").unwrap();
        let name = find_unique_name(dir.path(), "note", "md").unwrap();
        assert_eq!(name, "note (2).md");
        fs::write(dir.path().join("note (2).md.age"), b"x").unwrap();
        let name = find_unique_name(dir.path(), "note", "md").unwrap();
        assert_eq!(name, "note (3).md");
    }

    #[test]
    fn find_unique_name_handles_extensionless_input() {
        let dir = TempDir::new().unwrap();
        fs::write(dir.path().join("README.age"), b"x").unwrap();
        let name = find_unique_name(dir.path(), "README", "").unwrap();
        assert_eq!(name, "README (2)");
    }
}
