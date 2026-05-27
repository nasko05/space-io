use std::path::{Component, Path, PathBuf};
use std::time::SystemTime;

use age::secrecy::SecretString;
use time::OffsetDateTime;

use crate::crypto::age_io;
use crate::error::{AppError, AppResult};
use crate::space::Space;

const ENC_EXT: &str = ".age";

pub struct ReadFile {
    pub path: String,
    pub content: String,
    pub updated: Option<String>,
}

pub fn read_file(space: &Space, passphrase: &SecretString, rel_path: &str) -> AppResult<ReadFile> {
    let resolved = resolve(&space.root(), rel_path)?;
    let on_disk = with_age_suffix(&resolved);
    if !on_disk.is_file() {
        return Err(AppError::NotFound);
    }
    let bytes = std::fs::read(&on_disk)?;
    let plaintext = age_io::decrypt_bytes(&bytes, passphrase)?;
    let content = String::from_utf8(plaintext)
        .map_err(|_| AppError::Internal("non-utf8 note content".into()))?;
    let updated = std::fs::metadata(&on_disk)
        .and_then(|m| m.modified())
        .ok()
        .and_then(systemtime_iso8601);
    Ok(ReadFile {
        path: rel_path.to_string(),
        content,
        updated,
    })
}

/// Resolve `rel_path` against `root` while rejecting traversal (..),
/// absolute paths, and anything that escapes the root after normalisation.
fn resolve(root: &Path, rel: &str) -> AppResult<PathBuf> {
    let mut out = root.to_path_buf();
    let candidate = Path::new(rel);
    for comp in candidate.components() {
        match comp {
            Component::Normal(seg) => out.push(seg),
            // Any other component (ParentDir, RootDir, Prefix, CurDir) is rejected.
            _ => return Err(AppError::Forbidden),
        }
    }
    // Belt + braces: canonicalisation check. If the parent dir exists we use
    // it to confirm the resolved path remains under the canonical root.
    if let Ok(canonical_root) = root.canonicalize() {
        let canonical_target = match out.canonicalize() {
            Ok(p) => p,
            Err(_) => {
                // File may not yet exist; check the parent.
                let parent = out
                    .parent()
                    .ok_or(AppError::Forbidden)?
                    .canonicalize()
                    .map_err(|_| AppError::NotFound)?;
                if !parent.starts_with(&canonical_root) {
                    return Err(AppError::Forbidden);
                }
                return Ok(out);
            }
        };
        if !canonical_target.starts_with(&canonical_root) {
            return Err(AppError::Forbidden);
        }
    }
    Ok(out)
}

fn with_age_suffix(p: &Path) -> PathBuf {
    let mut s = p.as_os_str().to_owned();
    s.push(ENC_EXT);
    PathBuf::from(s)
}

fn systemtime_iso8601(t: SystemTime) -> Option<String> {
    let dt: OffsetDateTime = t.into();
    dt.format(&time::format_description::well_known::Rfc3339).ok()
}
