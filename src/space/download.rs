use age::secrecy::SecretString;

use crate::crypto::age_io;
use crate::error::{AppError, AppResult};
use crate::space::paths::{resolve_under, with_age_suffix};
use crate::space::Space;

#[derive(Debug)]
pub struct DownloadedFile {
    pub path: String,
    pub bytes: Vec<u8>,
}

pub fn fetch_decrypted(
    space: &Space,
    passphrase: &SecretString,
    rel_path: &str,
) -> AppResult<DownloadedFile> {
    let resolved = resolve_under(&space.root(), rel_path)?;
    let on_disk = with_age_suffix(&resolved);
    if !on_disk.is_file() {
        return Err(AppError::NotFound);
    }
    let bytes = std::fs::read(&on_disk)?;
    let plaintext = age_io::decrypt_bytes(&bytes, passphrase)?;
    Ok(DownloadedFile {
        path: rel_path.to_string(),
        bytes: plaintext,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::space::test_helpers::make_space;

    #[test]
    fn returns_decrypted_bytes() {
        let (_dir, space, pass) = make_space("p");
        crate::space::upload::store_upload(&space, &pass, "Up", "img.png", b"fake png bytes")
            .unwrap();
        let r = fetch_decrypted(&space, &pass, "Up/img.png").unwrap();
        assert_eq!(r.path, "Up/img.png");
        assert_eq!(r.bytes, b"fake png bytes");
    }

    #[test]
    fn missing_file_yields_not_found() {
        let (_dir, space, pass) = make_space("p");
        let err = fetch_decrypted(&space, &pass, "missing.png").unwrap_err();
        assert!(matches!(err, AppError::NotFound));
    }

    #[test]
    fn traversal_yields_forbidden() {
        let (_dir, space, pass) = make_space("p");
        let err = fetch_decrypted(&space, &pass, "../etc/passwd").unwrap_err();
        assert!(matches!(err, AppError::Forbidden));
    }

    #[test]
    fn wrong_passphrase_fails_to_decrypt() {
        let (_dir, space, pass) = make_space("right one");
        crate::space::upload::store_upload(&space, &pass, "", "x.bin", b"data").unwrap();
        let wrong = age::secrecy::SecretString::from("wrong one".to_string());
        let err = fetch_decrypted(&space, &wrong, "x.bin").unwrap_err();
        assert!(matches!(err, AppError::Internal(_)));
    }
}
