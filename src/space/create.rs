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
