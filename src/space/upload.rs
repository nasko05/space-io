use age::secrecy::SecretString;

use crate::crypto::age_io;
use crate::error::AppResult;
use crate::space::git::commit_all;
use crate::space::paths::{
    find_unique_name, resolve_under, sanitise_filename, split_stem_ext, with_age_suffix,
};
use crate::space::Space;

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

    let candidate = find_unique_name(&folder_resolved, &stem, &ext)?;

    let rel_path = if folder.is_empty() {
        candidate.clone()
    } else {
        format!("{}/{candidate}", folder.trim_end_matches('/'))
    };
    let on_disk = with_age_suffix(&folder_resolved.join(&candidate));
    let ciphertext = age_io::encrypt_bytes(bytes, passphrase)?;
    std::fs::write(&on_disk, &ciphertext)?;
    space.with_repo(|repo| commit_all(repo, &format!("Upload: {rel_path}")))?;

    Ok(UploadedFile {
        path: rel_path,
        size: bytes.len() as u64,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::error::AppError;
    use crate::space::test_helpers::make_space;

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
