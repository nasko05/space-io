use std::collections::BTreeMap;

use age::secrecy::SecretString;
use serde::{Deserialize, Serialize};

use crate::crypto::age_io;
use crate::error::{AppError, AppResult};
use crate::space::git::commit_all;
use crate::space::paths::with_age_suffix;
use crate::space::Space;

/// Per-file metadata (currently just tags). Lives in a single encrypted
/// index at the space root so a tag edit costs one decrypt + encrypt
/// rather than a sidecar per file.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct FileMeta {
    #[serde(default)]
    pub tags: Vec<String>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct MetaIndex {
    #[serde(default)]
    pub paths: BTreeMap<String, FileMeta>,
}

const META_REL: &str = ".space-meta.toml";

fn index_path(space: &Space) -> std::path::PathBuf {
    with_age_suffix(&space.root().join(META_REL))
}

pub fn load(space: &Space, passphrase: &SecretString) -> AppResult<MetaIndex> {
    let p = index_path(space);
    if !p.is_file() {
        return Ok(MetaIndex::default());
    }
    let bytes = std::fs::read(&p)?;
    let plaintext = age_io::decrypt_bytes(&bytes, passphrase)?;
    let text =
        String::from_utf8(plaintext).map_err(|_| AppError::Internal("non-utf8 meta".into()))?;
    toml::from_str(&text).map_err(|e| AppError::Internal(format!("parse meta: {e}")))
}

pub fn save(space: &Space, passphrase: &SecretString, index: &MetaIndex) -> AppResult<()> {
    let p = index_path(space);
    let text = toml::to_string_pretty(index)
        .map_err(|e| AppError::Internal(format!("serialize meta: {e}")))?;
    let ciphertext = age_io::encrypt_bytes(text.as_bytes(), passphrase)?;
    if let Some(parent) = p.parent() {
        std::fs::create_dir_all(parent)?;
    }
    std::fs::write(&p, &ciphertext)?;
    space.with_repo(|repo| commit_all(repo, "meta: update"))?;
    Ok(())
}

/// Replace the tags for `path`. Empty tags removes the entry.
pub fn set_tags(
    space: &Space,
    passphrase: &SecretString,
    path: &str,
    tags: Vec<String>,
) -> AppResult<()> {
    let mut idx = load(space, passphrase)?;
    let trimmed: Vec<String> = tags
        .into_iter()
        .map(|t| t.trim().to_string())
        .filter(|t| !t.is_empty())
        .collect();
    if trimmed.is_empty() {
        idx.paths.remove(path);
    } else {
        idx.paths.entry(path.to_string()).or_default().tags = trimmed;
    }
    save(space, passphrase, &idx)
}

/// Rewrite the index so any path under `from_prefix` (or equal to `from`)
/// migrates to `to_prefix` (resp. `to`). Used by rename/move/delete.
pub fn rewrite_paths(
    space: &Space,
    passphrase: &SecretString,
    from: &str,
    to: &str,
    is_directory: bool,
) -> AppResult<()> {
    let mut idx = load(space, passphrase)?;
    let mut changed = false;
    if is_directory {
        let from_prefix = format!("{from}/");
        let to_prefix = format!("{to}/");
        let keys: Vec<String> = idx
            .paths
            .keys()
            .filter(|k| k.starts_with(&from_prefix))
            .cloned()
            .collect();
        for old_key in keys {
            let new_key = old_key.replacen(&from_prefix, &to_prefix, 1);
            if let Some(entry) = idx.paths.remove(&old_key) {
                idx.paths.insert(new_key, entry);
                changed = true;
            }
        }
    } else if let Some(entry) = idx.paths.remove(from) {
        idx.paths.insert(to.to_string(), entry);
        changed = true;
    }
    if changed {
        save(space, passphrase, &idx)?;
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::space::test_helpers::make_space;

    #[test]
    fn empty_space_has_no_meta() {
        let (_d, s, p) = make_space("p");
        let idx = load(&s, &p).unwrap();
        assert!(idx.paths.is_empty());
    }

    #[test]
    fn set_tags_persists_and_roundtrips() {
        let (_d, s, p) = make_space("p");
        set_tags(&s, &p, "a.md", vec!["one".into(), "two".into()]).unwrap();
        let idx = load(&s, &p).unwrap();
        assert_eq!(idx.paths.len(), 1);
        assert_eq!(idx.paths["a.md"].tags, vec!["one", "two"]);
    }

    #[test]
    fn empty_tags_removes_entry() {
        let (_d, s, p) = make_space("p");
        set_tags(&s, &p, "a.md", vec!["one".into()]).unwrap();
        set_tags(&s, &p, "a.md", vec![]).unwrap();
        let idx = load(&s, &p).unwrap();
        assert!(!idx.paths.contains_key("a.md"));
    }

    #[test]
    fn whitespace_only_tags_are_dropped() {
        let (_d, s, p) = make_space("p");
        set_tags(&s, &p, "a.md", vec![" ok ".into(), "   ".into(), "".into()]).unwrap();
        let idx = load(&s, &p).unwrap();
        assert_eq!(idx.paths["a.md"].tags, vec!["ok"]);
    }

    #[test]
    fn rewrite_paths_renames_a_single_file() {
        let (_d, s, p) = make_space("p");
        set_tags(&s, &p, "a.md", vec!["t".into()]).unwrap();
        rewrite_paths(&s, &p, "a.md", "b.md", false).unwrap();
        let idx = load(&s, &p).unwrap();
        assert!(!idx.paths.contains_key("a.md"));
        assert_eq!(idx.paths["b.md"].tags, vec!["t"]);
    }

    #[test]
    fn rewrite_paths_migrates_folder_subtree() {
        let (_d, s, p) = make_space("p");
        set_tags(&s, &p, "Old/a.md", vec!["t1".into()]).unwrap();
        set_tags(&s, &p, "Old/sub/b.md", vec!["t2".into()]).unwrap();
        rewrite_paths(&s, &p, "Old", "New", true).unwrap();
        let idx = load(&s, &p).unwrap();
        assert!(!idx.paths.iter().any(|(k, _)| k.starts_with("Old/")));
        assert_eq!(idx.paths["New/a.md"].tags, vec!["t1"]);
        assert_eq!(idx.paths["New/sub/b.md"].tags, vec!["t2"]);
    }
}
