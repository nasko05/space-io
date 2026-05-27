use age::secrecy::SecretString;

use crate::crypto::age_io;
use crate::error::{AppError, AppResult};
use crate::space::git::commit_all;
use crate::space::paths::{resolve_under, with_age_suffix, ENC_EXT};
use crate::space::Space;

const MAX_FILENAME: usize = 200;

#[derive(Debug)]
pub struct UploadedFile {
    pub path: String,
    pub size: u64,
}

/// Encrypt `bytes` and write to `<folder>/<file_name>.age`. The file name is
/// sanitised; collisions get a `(2)`/`(3)`/… suffix before the extension.
/// Each file becomes its own commit.
pub fn store_upload(
    space: &Space,
    passphrase: &SecretString,
    folder: &str,
    file_name: &str,
    bytes: &[u8],
) -> AppResult<UploadedFile> {
    let root = space.root();
    let folder_resolved = resolve_under(&root, folder)?;
    std::fs::create_dir_all(&folder_resolved)?;

    let safe_name = sanitise_filename(file_name)?;
    let (stem, ext) = split_stem_ext(&safe_name);

    let mut candidate = safe_name.clone();
    let mut counter = 2;
    while folder_resolved
        .join(format!("{candidate}{ENC_EXT}"))
        .exists()
    {
        candidate = if ext.is_empty() {
            format!("{stem} ({counter})")
        } else {
            format!("{stem} ({counter}).{ext}")
        };
        counter += 1;
        if counter > 999 {
            return Err(AppError::Internal("too many filename collisions".into()));
        }
    }

    let rel_path = if folder.is_empty() {
        candidate.clone()
    } else {
        format!("{}/{candidate}", folder.trim_end_matches('/'))
    };
    let on_disk = with_age_suffix(&folder_resolved.join(&candidate));
    let ciphertext = age_io::encrypt_bytes(bytes, passphrase)?;
    std::fs::write(&on_disk, &ciphertext)?;
    commit_all(&root, &format!("Upload: {rel_path}"))?;

    Ok(UploadedFile {
        path: rel_path,
        size: bytes.len() as u64,
    })
}

fn sanitise_filename(input: &str) -> AppResult<String> {
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

fn split_stem_ext(name: &str) -> (String, String) {
    match name.rfind('.') {
        Some(i) if i > 0 && i < name.len() - 1 => {
            (name[..i].to_string(), name[i + 1..].to_string())
        }
        _ => (name.to_string(), String::new()),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::space::test_helpers::make_space;

    #[test]
    fn sanitise_passes_simple_names() {
        assert_eq!(sanitise_filename("ok.txt").unwrap(), "ok.txt");
    }

    #[test]
    fn sanitise_rejects_path_separators() {
        assert!(sanitise_filename("a/b").is_err());
        assert!(sanitise_filename("a\\b").is_err());
        assert!(sanitise_filename("a\0b").is_err());
    }

    #[test]
    fn sanitise_rejects_dotfiles() {
        assert!(sanitise_filename(".env").is_err());
    }

    #[test]
    fn sanitise_rejects_empty() {
        assert!(sanitise_filename("").is_err());
        assert!(sanitise_filename("   ").is_err());
    }

    #[test]
    fn sanitise_truncates_long_names() {
        let long = "a".repeat(500);
        let result = sanitise_filename(&long).unwrap();
        assert!(result.len() <= MAX_FILENAME);
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
        // .gitignore-style names — there's nothing before the dot
        assert_eq!(split_stem_ext(".hidden"), (".hidden".into(), String::new()));
        // trailing dot
        assert_eq!(split_stem_ext("foo."), ("foo.".into(), String::new()));
    }

    #[test]
    fn store_upload_writes_encrypted_bytes_under_folder() {
        let (dir, space, pass) = make_space("p");
        let r = store_upload(&space, &pass, "Documents", "lease.pdf", b"binary").unwrap();
        assert_eq!(r.path, "Documents/lease.pdf");
        assert_eq!(r.size, 6);
        let on_disk = dir.path().join("space/Documents/lease.pdf.age");
        assert!(on_disk.is_file());
        let bytes = std::fs::read(&on_disk).unwrap();
        assert!(bytes.starts_with(b"age-encryption.org/v1\n"));
        let pt = crate::crypto::age_io::decrypt_bytes(&bytes, &pass).unwrap();
        assert_eq!(pt, b"binary");
    }

    #[test]
    fn store_upload_collision_gets_paren_suffix() {
        let (_dir, space, pass) = make_space("p");
        let a = store_upload(&space, &pass, "Up", "x.png", b"a").unwrap();
        let b = store_upload(&space, &pass, "Up", "x.png", b"b").unwrap();
        assert_eq!(a.path, "Up/x.png");
        assert_eq!(b.path, "Up/x (2).png");
    }

    #[test]
    fn store_upload_collision_preserves_no_extension() {
        let (_dir, space, pass) = make_space("p");
        let a = store_upload(&space, &pass, "Up", "README", b"a").unwrap();
        let b = store_upload(&space, &pass, "Up", "README", b"b").unwrap();
        assert_eq!(a.path, "Up/README");
        assert_eq!(b.path, "Up/README (2)");
    }

    #[test]
    fn store_upload_rejects_traversal_folder() {
        let (_dir, space, pass) = make_space("p");
        let err = store_upload(&space, &pass, "../etc", "x.png", b"x").unwrap_err();
        assert!(matches!(err, AppError::Forbidden));
    }

    #[test]
    fn store_upload_rejects_filename_with_slash() {
        let (_dir, space, pass) = make_space("p");
        let err = store_upload(&space, &pass, "Up", "evil/escape.png", b"x").unwrap_err();
        assert!(matches!(err, AppError::BadRequest(_)));
    }

    #[test]
    fn store_upload_empty_folder_writes_to_root() {
        let (dir, space, pass) = make_space("p");
        let r = store_upload(&space, &pass, "", "loose.png", b"x").unwrap();
        assert_eq!(r.path, "loose.png");
        assert!(dir.path().join("space/loose.png.age").is_file());
    }
}
