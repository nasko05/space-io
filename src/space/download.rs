use age::secrecy::SecretString;

use crate::crypto::age_io;
use crate::error::{AppError, AppResult};
use crate::space::paths::{resolve_under, with_age_suffix};
use crate::space::Space;

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
