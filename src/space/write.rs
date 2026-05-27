use age::secrecy::SecretString;

use crate::crypto::age_io;
use crate::error::AppResult;
use crate::space::git::commit_all;
use crate::space::paths::{resolve_under, with_age_suffix};
use crate::space::Space;

pub struct WriteResult {
    pub path: String,
    pub updated: String,
}

pub fn write_file(
    space: &Space,
    passphrase: &SecretString,
    rel_path: &str,
    content: &str,
    message: Option<&str>,
) -> AppResult<WriteResult> {
    let root = space.root();
    let resolved = resolve_under(&root, rel_path)?;
    if let Some(parent) = resolved.parent() {
        std::fs::create_dir_all(parent)?;
    }
    let on_disk = with_age_suffix(&resolved);

    let ciphertext = age_io::encrypt_bytes(content.as_bytes(), passphrase)?;
    std::fs::write(&on_disk, &ciphertext)?;

    let summary = message
        .map(|m| m.to_string())
        .unwrap_or_else(|| format!("Edit: {rel_path}"));
    commit_all(&root, &summary)?;

    let updated = std::fs::metadata(&on_disk)
        .and_then(|m| m.modified())
        .ok()
        .and_then(|t| {
            let dt: time::OffsetDateTime = t.into();
            dt.format(&time::format_description::well_known::Rfc3339)
                .ok()
        })
        .unwrap_or_default();

    Ok(WriteResult {
        path: rel_path.to_string(),
        updated,
    })
}
