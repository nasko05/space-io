use std::collections::BTreeMap;
use std::sync::Arc;

use age::secrecy::SecretString;
use walkdir::WalkDir;

use crate::crypto::age_io;
use crate::error::AppResult;
use crate::space::paths::ENC_EXT;
use crate::space::Space;

pub struct Excerpt {
    pub title: Option<String>,
    pub excerpt: String,
}

const EXCERPT_CHARS: usize = 180;

/// Walk every `.md.age` file, decrypt (with cache), and return a `path →
/// excerpt` map. Title is the first `# ...` line; excerpt is the first ~180
/// chars of body. Cached plaintext keyed by `path + mtime` keeps repeated
/// excerpt rebuilds cheap.
pub fn build_excerpts(
    space: &Space,
    passphrase: &SecretString,
) -> AppResult<BTreeMap<String, Excerpt>> {
    let root = space.root();
    let mut out = BTreeMap::new();
    if !root.is_dir() {
        return Ok(out);
    }
    let cache = space.cache();

    for entry in WalkDir::new(&root).into_iter().filter_map(Result::ok) {
        if !entry.file_type().is_file() {
            continue;
        }
        let path = entry.path();
        let file_name = match path.file_name().and_then(|n| n.to_str()) {
            Some(n) => n,
            None => continue,
        };
        let Some(visible) = file_name.strip_suffix(ENC_EXT) else {
            continue;
        };
        if !visible.to_ascii_lowercase().ends_with(".md") {
            continue;
        }

        let rel = match path.strip_prefix(&root) {
            Ok(r) => r,
            Err(_) => continue,
        };
        let visible_rel = rel
            .to_string_lossy()
            .replace('\\', "/")
            .trim_end_matches(ENC_EXT)
            .to_string();

        let mtime = match entry.metadata().ok().and_then(|m| m.modified().ok()) {
            Some(t) => t,
            None => continue,
        };
        let cache_key = path.to_string_lossy().into_owned();
        let text = if let Some(cached) = cache.get(&cache_key, mtime) {
            cached
        } else {
            let bytes = match std::fs::read(path) {
                Ok(b) => b,
                Err(_) => continue,
            };
            let plaintext = match age_io::decrypt_bytes(&bytes, passphrase) {
                Ok(p) => p,
                Err(_) => continue,
            };
            let Ok(text) = String::from_utf8(plaintext) else {
                continue;
            };
            let arc: Arc<str> = Arc::from(text);
            cache.put(cache_key.clone(), mtime, arc.clone());
            arc
        };

        let title = extract_title(&text);
        let excerpt = extract_excerpt(&text);
        out.insert(visible_rel, Excerpt { title, excerpt });
    }
    Ok(out)
}

/// Pull the first `# …` heading from markdown source as the title.
pub fn extract_title(src: &str) -> Option<String> {
    src.lines()
        .find_map(|l| l.strip_prefix("# ").map(|t| t.trim().to_string()))
}

fn extract_excerpt(src: &str) -> String {
    let body: String = src
        .lines()
        .filter(|l| !l.starts_with('#') && !l.trim().is_empty())
        .take(3)
        .collect::<Vec<_>>()
        .join(" ");
    let cleaned = body
        .replace(['*', '_', '`'], "")
        .replace("[[", "")
        .replace("]]", "");
    if cleaned.chars().count() <= EXCERPT_CHARS {
        cleaned
    } else {
        let truncated: String = cleaned.chars().take(EXCERPT_CHARS).collect();
        format!("{}…", truncated.trim_end())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::space::test_helpers::make_space_with_note;

    #[test]
    fn extract_title_finds_first_h1() {
        assert_eq!(extract_title("# Hello\n\nbody"), Some("Hello".to_string()));
    }

    #[test]
    fn extract_title_returns_none_when_no_h1() {
        assert!(extract_title("just a body").is_none());
    }

    #[test]
    fn extract_title_ignores_h2() {
        assert!(extract_title("## sub").is_none());
    }

    #[test]
    fn extract_excerpt_strips_emphasis_markers() {
        let s = "# T\n\nA *star* and **bold** word.";
        assert!(!extract_excerpt(s).contains('*'));
    }

    #[test]
    fn extract_excerpt_strips_wikilink_brackets() {
        let s = "# T\n\nSee [[Memory palaces]] for more.";
        let ex = extract_excerpt(s);
        assert!(ex.contains("Memory palaces"));
        assert!(!ex.contains("[["));
    }

    #[test]
    fn extract_excerpt_truncates_with_ellipsis() {
        let long = "x".repeat(500);
        let s = format!("# T\n\n{long}");
        let ex = extract_excerpt(&s);
        assert!(ex.ends_with('…'));
    }

    #[test]
    fn build_excerpts_returns_one_entry_per_md_file() {
        let (_dir, space, pass) = make_space_with_note("p", "Journal/a.md", "# A\n\nBody A.");
        crate::space::write::write_file(&space, &pass, "Journal/b.md", "# B\n\nBody B.", None)
            .unwrap();
        let map = build_excerpts(&space, &pass).unwrap();
        assert_eq!(map.len(), 2);
        assert_eq!(map["Journal/a.md"].title.as_deref(), Some("A"));
        assert_eq!(map["Journal/b.md"].title.as_deref(), Some("B"));
        assert!(map["Journal/a.md"].excerpt.contains("Body A"));
    }

    #[test]
    fn build_excerpts_ignores_non_markdown() {
        let (dir, space, pass) = make_space_with_note("p", "Note.md", "# N\n\nbody");
        // Drop a fake encrypted pdf next door — should be skipped.
        crate::space::upload::store_upload(&space, &pass, "", "scan.pdf", b"%PDF-1.4 minimal")
            .unwrap();
        let map = build_excerpts(&space, &pass).unwrap();
        assert_eq!(map.len(), 1, "only .md is returned");
        assert!(map.contains_key("Note.md"));
        let _ = dir;
    }
}
