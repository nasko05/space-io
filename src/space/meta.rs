use std::collections::BTreeMap;
use std::sync::{Arc, Mutex};

use age::secrecy::SecretString;
use serde::{Deserialize, Serialize};

use crate::crypto::age_io;
use crate::error::{AppError, AppResult};
use crate::space::git::commit_paths;
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

/// In-memory cache for the decrypted meta index. The meta file is read on
/// every tag edit, every search, and on every rename/delete to update path
/// keys. Each load pays a full age-passphrase decrypt (scrypt-derived KDF,
/// deliberately slow), so caching the parsed index avoids hundreds of
/// milliseconds of CPU per request.
///
/// The cache stays consistent with disk because every successful `save`
/// replaces the cached value with the freshly-persisted index.
#[derive(Clone, Default)]
pub struct MetaCache {
    inner: Arc<Mutex<Option<Arc<MetaIndex>>>>,
}

impl MetaCache {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn read(&self) -> Option<Arc<MetaIndex>> {
        self.inner.lock().ok().and_then(|g| g.clone())
    }

    pub fn replace(&self, idx: Arc<MetaIndex>) {
        if let Ok(mut g) = self.inner.lock() {
            *g = Some(idx);
        }
    }
}

const META_REL: &str = ".space-meta.toml";

fn index_path(space: &Space) -> std::path::PathBuf {
    with_age_suffix(&space.root().join(META_REL))
}

/// Load the meta index. First call after a lock pays the age decrypt; later
/// calls return the cached `Arc<MetaIndex>` until a `save` replaces it.
pub fn load(space: &Space, passphrase: &SecretString) -> AppResult<Arc<MetaIndex>> {
    if let Some(cached) = space.meta_cache().read() {
        return Ok(cached);
    }
    let p = index_path(space);
    let idx = if !p.is_file() {
        MetaIndex::default()
    } else {
        let bytes = std::fs::read(&p)?;
        let plaintext = age_io::decrypt_bytes(&bytes, passphrase)?;
        let text =
            String::from_utf8(plaintext).map_err(|_| AppError::Internal("non-utf8 meta".into()))?;
        toml::from_str(&text).map_err(|e| AppError::Internal(format!("parse meta: {e}")))?
    };
    let arc = Arc::new(idx);
    space.meta_cache().replace(arc.clone());
    Ok(arc)
}

/// Encrypt + write the index to disk and refresh the cache. Does NOT
/// commit. Use this when the caller wants to bundle the meta change with
/// another filesystem change (e.g. a file rename) into a single commit;
/// otherwise prefer `save`.
pub fn write_index(space: &Space, passphrase: &SecretString, index: &MetaIndex) -> AppResult<()> {
    let p = index_path(space);
    let text = toml::to_string_pretty(index)
        .map_err(|e| AppError::Internal(format!("serialize meta: {e}")))?;
    let ciphertext = age_io::encrypt_bytes(text.as_bytes(), passphrase)?;
    if let Some(parent) = p.parent() {
        std::fs::create_dir_all(parent)?;
    }
    std::fs::write(&p, &ciphertext)?;
    space.meta_cache().replace(Arc::new(index.clone()));
    Ok(())
}

/// Write the index to disk, commit "meta: update", and refresh the cache.
/// The default save path for tag operations where the meta change is the
/// only thing happening. Staging is narrowed to the meta blob so the
/// commit doesn't scan the entire working tree.
pub fn save(space: &Space, passphrase: &SecretString, index: &MetaIndex) -> AppResult<()> {
    write_index(space, passphrase, index)?;
    space.with_repo(|repo| {
        commit_paths(
            repo,
            "meta: update",
            [std::path::PathBuf::from(META_BLOB_REL)],
        )
    })
}

const META_BLOB_REL: &str = ".space-meta.toml.age";

/// Apply a sequence of `(path, tags)` updates atomically: one load, one
/// save, one commit, regardless of how many files are touched. Empty tags
/// removes the entry for that path. Whitespace-only tags are dropped.
pub fn set_tags_bulk(
    space: &Space,
    passphrase: &SecretString,
    updates: Vec<(String, Vec<String>)>,
) -> AppResult<()> {
    if updates.is_empty() {
        return Ok(());
    }
    let cached = load(space, passphrase)?;
    let mut idx: MetaIndex = (*cached).clone();
    let mut changed = false;
    for (path, tags) in updates {
        let trimmed: Vec<String> = tags
            .into_iter()
            .map(|t| t.trim().to_string())
            .filter(|t| !t.is_empty())
            .collect();
        if trimmed.is_empty() {
            if idx.paths.remove(&path).is_some() {
                changed = true;
            }
        } else {
            let entry = idx.paths.entry(path).or_default();
            if entry.tags != trimmed {
                entry.tags = trimmed;
                changed = true;
            }
        }
    }
    if changed {
        save(space, passphrase, &idx)?;
    }
    Ok(())
}

/// Replace the tags for `path`. Empty tags removes the entry.
pub fn set_tags(
    space: &Space,
    passphrase: &SecretString,
    path: &str,
    tags: Vec<String>,
) -> AppResult<()> {
    set_tags_bulk(space, passphrase, vec![(path.to_string(), tags)])
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::space::test_helpers::{count_commits, make_space};

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
    fn set_tags_bulk_applies_all_updates_in_one_commit() {
        let (d, s, p) = make_space("p");
        crate::space::write::write_file(&s, &p, "a.md", "x", None).unwrap();
        crate::space::write::write_file(&s, &p, "b.md", "y", None).unwrap();
        crate::space::write::write_file(&s, &p, "c.md", "z", None).unwrap();

        let commits_before = count_commits(&d.path().join("space"));
        set_tags_bulk(
            &s,
            &p,
            vec![
                ("a.md".into(), vec!["one".into()]),
                ("b.md".into(), vec!["two".into()]),
                ("c.md".into(), vec!["three".into()]),
            ],
        )
        .unwrap();
        let commits_after = count_commits(&d.path().join("space"));
        assert_eq!(
            commits_after - commits_before,
            1,
            "bulk set_tags should produce exactly one commit",
        );

        let idx = load(&s, &p).unwrap();
        assert_eq!(idx.paths["a.md"].tags, vec!["one"]);
        assert_eq!(idx.paths["b.md"].tags, vec!["two"]);
        assert_eq!(idx.paths["c.md"].tags, vec!["three"]);
    }

    #[test]
    fn set_tags_bulk_empty_input_is_a_noop() {
        let (d, s, p) = make_space("p");
        let before = count_commits(&d.path().join("space"));
        set_tags_bulk(&s, &p, vec![]).unwrap();
        assert_eq!(count_commits(&d.path().join("space")), before);
    }

    #[test]
    fn set_tags_bulk_skips_commit_when_nothing_changed() {
        let (d, s, p) = make_space("p");
        set_tags(&s, &p, "a.md", vec!["same".into()]).unwrap();
        let before = count_commits(&d.path().join("space"));
        set_tags_bulk(&s, &p, vec![("a.md".into(), vec!["same".into()])]).unwrap();
        assert_eq!(count_commits(&d.path().join("space")), before);
    }

    #[test]
    fn cache_makes_repeated_loads_return_same_arc() {
        let (_d, s, p) = make_space("p");
        set_tags(&s, &p, "a.md", vec!["t".into()]).unwrap();
        let a = load(&s, &p).unwrap();
        let b = load(&s, &p).unwrap();
        assert!(
            Arc::ptr_eq(&a, &b),
            "cached load should return the same Arc",
        );
    }

    #[test]
    fn save_invalidates_cache_with_new_value() {
        let (_d, s, p) = make_space("p");
        set_tags(&s, &p, "a.md", vec!["one".into()]).unwrap();
        let first = load(&s, &p).unwrap();
        set_tags(&s, &p, "a.md", vec!["two".into()]).unwrap();
        let second = load(&s, &p).unwrap();
        assert!(!Arc::ptr_eq(&first, &second));
        assert_eq!(second.paths["a.md"].tags, vec!["two"]);
    }
}
