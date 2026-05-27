use age::secrecy::SecretString;

use crate::crypto::age_io;
use crate::error::{AppError, AppResult};
use crate::space::git::commit_all;
use crate::space::paths::{resolve_under, with_age_suffix, ENC_EXT};
use crate::space::Space;

const MAX_FILENAME: usize = 200;

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
        return Err(AppError::BadRequest("filename cannot start with '.'".into()));
    }
    if out.len() > MAX_FILENAME {
        out.truncate(MAX_FILENAME);
    }
    Ok(out)
}

fn split_stem_ext(name: &str) -> (String, String) {
    match name.rfind('.') {
        Some(i) if i > 0 && i < name.len() - 1 => (name[..i].to_string(), name[i + 1..].to_string()),
        _ => (name.to_string(), String::new()),
    }
}
