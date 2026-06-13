//! Crash-safe file writes.
//!
//! `std::fs::write` truncates then streams, so a crash mid-write leaves a torn
//! file — catastrophic for the metadata that gates access to a space
//! (`.users.toml`, `.space.toml`). `write_atomic` writes to a sibling temp
//! file, fsyncs, and atomically `rename`s it over the destination, so readers
//! and post-crash restarts see either the old file or the complete new one.

use std::fs;
use std::io::Write;
use std::path::{Path, PathBuf};

/// Atomically replace `path`'s contents, creating the parent directory if
/// needed. The temp file shares the destination's directory so the final
/// `rename` never crosses a filesystem boundary (`EXDEV`).
pub fn write_atomic(path: &Path, contents: &[u8]) -> std::io::Result<()> {
    let parent = path.parent().unwrap_or_else(|| Path::new("."));
    fs::create_dir_all(parent)?;

    let tmp = tmp_path(path);
    {
        let mut file = fs::File::create(&tmp)?;
        file.write_all(contents)?;
        file.sync_all()?;
    }
    if let Err(e) = fs::rename(&tmp, path) {
        let _ = fs::remove_file(&tmp);
        return Err(e);
    }
    Ok(())
}

/// `foo/bar.toml` → `foo/bar.toml.tmp`. A fixed suffix is safe: writers are
/// serialised by a lock and `File::create` truncates any stale temp.
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
