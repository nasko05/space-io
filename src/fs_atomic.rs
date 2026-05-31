//! Crash-safe file writes.
//!
//! `std::fs::write` truncates the destination and then streams the new bytes,
//! so a crash (or power loss) mid-write leaves a half-written file. For the
//! tiny-but-critical metadata files we keep at rest — `.users.toml` (the
//! email→UUID map that makes every space reachable) and each space's
//! `.space.toml` — a torn write is catastrophic: it can lock a user out of
//! their encrypted data permanently.
//!
//! `write_atomic` instead writes the full contents to a sibling temp file,
//! fsyncs it, and `rename`s it over the destination. POSIX `rename` is atomic,
//! so a concurrent reader (or a restart after a crash) sees either the old
//! file or the complete new one — never a truncation.

use std::fs;
use std::io::Write;
use std::path::{Path, PathBuf};

/// Atomically replace `path`'s contents with `contents`.
///
/// Creates the parent directory if needed. The temp file lives in the same
/// directory as the destination so the final `rename` never crosses a
/// filesystem boundary (which would fail with `EXDEV`).
pub fn write_atomic(path: &Path, contents: &[u8]) -> std::io::Result<()> {
    let parent = path.parent().unwrap_or_else(|| Path::new("."));
    fs::create_dir_all(parent)?;

    let tmp = tmp_path(path);
    {
        let mut file = fs::File::create(&tmp)?;
        file.write_all(contents)?;
        file.sync_all()?;
    }
    // Replace the destination. On failure, don't leave the temp behind.
    if let Err(e) = fs::rename(&tmp, path) {
        let _ = fs::remove_file(&tmp);
        return Err(e);
    }
    Ok(())
}

/// `foo/bar.toml` → `foo/bar.toml.tmp`. Same directory, distinctive suffix.
/// In-process writers to a given file are already serialised by a lock, and
/// `File::create` truncates any stale temp left by a previous crash, so a
/// fixed suffix is safe here.
fn tmp_path(path: &Path) -> PathBuf {
    let mut name = path
        .file_name()
        .map(|n| n.to_os_string())
        .unwrap_or_default();
    name.push(".tmp");
    path.with_file_name(name)
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    #[test]
    fn writes_then_reads_back() {
        let dir = TempDir::new().unwrap();
        let path = dir.path().join("data.txt");
        write_atomic(&path, b"hello").unwrap();
        assert_eq!(fs::read(&path).unwrap(), b"hello");
    }

    #[test]
    fn overwrites_existing_file() {
        let dir = TempDir::new().unwrap();
        let path = dir.path().join("data.txt");
        write_atomic(&path, b"first").unwrap();
        write_atomic(&path, b"second").unwrap();
        assert_eq!(fs::read(&path).unwrap(), b"second");
    }

    #[test]
    fn creates_missing_parent_dirs() {
        let dir = TempDir::new().unwrap();
        let path = dir.path().join("nested/deeper/data.txt");
        write_atomic(&path, b"x").unwrap();
        assert_eq!(fs::read(&path).unwrap(), b"x");
    }

    #[test]
    fn leaves_no_temp_file_behind() {
        let dir = TempDir::new().unwrap();
        let path = dir.path().join("data.txt");
        write_atomic(&path, b"x").unwrap();
        assert!(!dir.path().join("data.txt.tmp").exists());
    }
}
