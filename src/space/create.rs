use age::secrecy::SecretString;
use time::OffsetDateTime;

use crate::crypto::age_io;
use crate::error::{AppError, AppResult};
use crate::space::git::commit_all;
use crate::space::paths::{resolve_under, with_age_suffix, ENC_EXT};
use crate::space::Space;

/// Sanitise an optional title into a filesystem-safe stem. Empty input yields
/// `None`, signalling the caller should fall back to a time-based name.
fn sanitise(title: &str) -> Option<String> {
    let trimmed = title.trim();
    if trimmed.is_empty() {
        return None;
    }
    let mut out = String::with_capacity(trimmed.len());
    for ch in trimmed.chars() {
        if ch.is_ascii_alphanumeric() || ch == ' ' || ch == '-' || ch == '_' {
            out.push(ch);
        } else if ch == ',' || ch == '.' || ch == '!' || ch == '?' {
            // skip
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

#[derive(Debug)]
pub struct CreateResult {
    pub path: String,
}

pub fn create_file(
    space: &Space,
    passphrase: &SecretString,
    folder: &str,
    title: Option<&str>,
) -> AppResult<CreateResult> {
    let root = space.root();
    let folder_resolved = resolve_under(&root, folder)?;
    std::fs::create_dir_all(&folder_resolved)?;

    let now = OffsetDateTime::now_utc();
    let stem = title
        .and_then(sanitise)
        .unwrap_or_else(|| format!("Untitled {:02}-{:02}", now.hour(), now.minute()));

    // Find a non-colliding filename.
    let mut filename = format!("{stem}.md");
    let mut counter = 2;
    while folder_resolved
        .join(format!("{filename}{ENC_EXT}"))
        .exists()
    {
        filename = format!("{stem} ({counter}).md");
        counter += 1;
        if counter > 999 {
            return Err(AppError::Internal("too many name collisions".into()));
        }
    }

    let rel_path = if folder.is_empty() {
        filename.clone()
    } else {
        format!("{}/{filename}", folder.trim_end_matches('/'))
    };

    let initial = String::new();
    let on_disk = with_age_suffix(&folder_resolved.join(&filename));
    let ciphertext = age_io::encrypt_bytes(initial.as_bytes(), passphrase)?;
    std::fs::write(&on_disk, &ciphertext)?;
    commit_all(&root, &format!("Create: {rel_path}"))?;

    Ok(CreateResult { path: rel_path })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::space::test_helpers::make_space;

    #[test]
    fn sanitise_preserves_simple_titles() {
        assert_eq!(sanitise("My Note"), Some("My Note".into()));
    }

    #[test]
    fn sanitise_strips_punctuation() {
        assert_eq!(sanitise("Hello, world!"), Some("Hello world".into()));
    }

    #[test]
    fn sanitise_replaces_unsupported_chars_with_dash() {
        assert_eq!(sanitise("a/b\\c"), Some("a-b-c".into()));
    }

    #[test]
    fn sanitise_returns_none_for_empty() {
        assert_eq!(sanitise(""), None);
        assert_eq!(sanitise("   "), None);
    }

    #[test]
    fn sanitise_collapses_whitespace() {
        assert_eq!(sanitise("hello   world"), Some("hello world".into()));
    }

    #[test]
    fn untitled_when_no_title_is_provided() {
        let (_dir, space, pass) = make_space("p");
        let r = create_file(&space, &pass, "Journal/2026", None).unwrap();
        let name = r.path.split('/').next_back().unwrap();
        assert!(
            name.starts_with("Untitled "),
            "expected 'Untitled ...', got {}",
            name
        );
        assert!(name.ends_with(".md"));
    }

    #[test]
    fn title_seeds_filename_stem() {
        let (_dir, space, pass) = make_space("p");
        let r = create_file(&space, &pass, "Journal/2026", Some("Memo for M")).unwrap();
        assert_eq!(r.path, "Journal/2026/Memo for M.md");
    }

    #[test]
    fn collision_yields_paren_suffix() {
        let (_dir, space, pass) = make_space("p");
        let a = create_file(&space, &pass, "Journal/2026", Some("Note")).unwrap();
        let b = create_file(&space, &pass, "Journal/2026", Some("Note")).unwrap();
        let c = create_file(&space, &pass, "Journal/2026", Some("Note")).unwrap();
        assert_eq!(a.path, "Journal/2026/Note.md");
        assert_eq!(b.path, "Journal/2026/Note (2).md");
        assert_eq!(c.path, "Journal/2026/Note (3).md");
    }

    #[test]
    fn create_produces_an_encrypted_empty_file() {
        let (dir, space, pass) = make_space("p");
        let r = create_file(&space, &pass, "F", Some("a")).unwrap();
        let on_disk = dir.path().join("space").join(format!("{}.age", r.path));
        let bytes = std::fs::read(&on_disk).unwrap();
        assert!(bytes.starts_with(b"age-encryption.org/v1\n"));
        let pt = crate::crypto::age_io::decrypt_bytes(&bytes, &pass).unwrap();
        assert!(pt.is_empty());
    }

    #[test]
    fn rejects_traversal_folder() {
        let (_dir, space, pass) = make_space("p");
        let err = create_file(&space, &pass, "../etc", None).unwrap_err();
        assert!(matches!(err, AppError::Forbidden));
    }

    #[test]
    fn empty_folder_drops_path_to_root() {
        let (dir, space, pass) = make_space("p");
        let r = create_file(&space, &pass, "", Some("root note")).unwrap();
        assert_eq!(r.path, "root note.md");
        assert!(dir.path().join("space/root note.md.age").is_file());
    }
}
