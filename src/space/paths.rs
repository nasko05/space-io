use std::path::{Component, Path, PathBuf};

use crate::error::{AppError, AppResult};

pub const ENC_EXT: &str = ".age";

/// Resolve a relative path against `root`, rejecting `..`, absolute paths,
/// and anything that escapes root after canonicalisation.
pub fn resolve_under(root: &Path, rel: &str) -> AppResult<PathBuf> {
    let mut out = root.to_path_buf();
    let candidate = Path::new(rel);
    for comp in candidate.components() {
        match comp {
            Component::Normal(seg) => out.push(seg),
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
                if let Some(parent) = out.parent() {
                    if parent.exists() {
                        let canonical_parent = parent
                            .canonicalize()
                            .map_err(|_| AppError::Forbidden)?;
                        if !canonical_parent.starts_with(&canonical_root) {
                            return Err(AppError::Forbidden);
                        }
                    } else if !parent.starts_with(&canonical_root) {
                        // Parent doesn't exist yet; ensure the prefix stays
                        // inside root by comparing the resolved buffer.
                        if !out.starts_with(&canonical_root) {
                            return Err(AppError::Forbidden);
                        }
                    }
                } else {
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
