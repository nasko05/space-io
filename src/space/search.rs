use std::sync::Arc;

use age::secrecy::SecretString;
use walkdir::WalkDir;

use crate::crypto::age_io;
use crate::error::AppResult;
use crate::space::excerpt::{clean_markup, extract_title};
use crate::space::paths::ENC_EXT;
use crate::space::Space;

const SNIPPET_CHARS: usize = 140;
const MAX_HITS: usize = 24;

pub struct SearchHit {
    pub path: String,
    pub title: Option<String>,
    pub snippet: String,
    pub score: usize,
}

/// Decrypt every `.md.age`, look for the query (whitespace-tokenised AND
/// semantics, case-insensitive), and return up to `MAX_HITS` ordered by a
/// tiny score: 3 points for a title hit, 1 point per body match.
///
/// We deliberately walk-and-grep instead of maintaining a persistent
/// tantivy index — keeping the index in memory only would mean a startup
/// rebuild for every restart, and persisting it would leak plaintext
/// titles/bodies to disk in violation of the SPEC's "server stores
/// ciphertext only" guarantee. Plaintext is cached in memory per
/// (path, mtime) so repeat searches don't re-decrypt unchanged files.
/// Re-evaluate if the corpus crosses ~10k notes.
pub fn search(space: &Space, passphrase: &SecretString, query: &str) -> AppResult<Vec<SearchHit>> {
    let tokens: Vec<String> = query
        .split_whitespace()
        .map(|t| t.to_ascii_lowercase())
        .filter(|t| !t.is_empty())
        .collect();
    if tokens.is_empty() {
        return Ok(vec![]);
    }

    let root = space.root();
    if !root.is_dir() {
        return Ok(vec![]);
    }
    let cache = space.cache();

    // Tags live in a single encrypted index at the root; load it once so each
    // per-file tag check is an in-memory lookup, not another decrypt.
    let meta_index = crate::space::meta::load(space, passphrase)
        .unwrap_or_else(|_| std::sync::Arc::new(crate::space::meta::MetaIndex::default()));

    let mut hits: Vec<SearchHit> = Vec::new();

    for entry in WalkDir::new(&root).into_iter().filter_map(Result::ok) {
        if !entry.file_type().is_file() {
            continue;
        }
        let path = entry.path();
        let Some(file_name) = path.file_name().and_then(|n| n.to_str()) else {
            continue;
        };
        let Some(visible) = file_name.strip_suffix(ENC_EXT) else {
            continue;
        };
        if !visible.to_ascii_lowercase().ends_with(".md") {
            continue;
        }

        let Ok(rel) = path.strip_prefix(&root) else {
            continue;
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
        let (text, lower): (Arc<str>, Arc<str>) =
            if let Some(pair) = cache.get_with_lowered(&cache_key, mtime) {
                pair
            } else {
                let Ok(bytes) = std::fs::read(path) else {
                    continue;
                };
                let Ok(plaintext) = age_io::decrypt_bytes(&bytes, passphrase) else {
                    continue;
                };
                let Ok(text) = String::from_utf8(plaintext) else {
                    continue;
                };
                let arc: Arc<str> = Arc::from(text);
                cache.put(cache_key.clone(), mtime, arc.clone());
                // Now ask the cache for the lowered mirror so it's stored
                // for future searches as well.
                match cache.get_with_lowered(&cache_key, mtime) {
                    Some(pair) => pair,
                    None => (arc.clone(), Arc::from(arc.to_ascii_lowercase())),
                }
            };

        let title = extract_title(&text);
        let title_lower = title.as_deref().map(|t| t.to_ascii_lowercase());
        let tags_lower: Vec<String> = meta_index
            .paths
            .get(&visible_rel)
            .map(|m| m.tags.iter().map(|t| t.to_ascii_lowercase()).collect())
            .unwrap_or_default();

        let mut score = 0usize;
        let mut first_idx: Option<usize> = None;
        let mut all_match = true;
        for tok in &tokens {
            let body_match = lower.find(tok);
            let title_match = title_lower.as_deref().and_then(|t| t.find(tok));
            let tag_match = tags_lower.iter().any(|t| t.contains(tok));
            if body_match.is_none() && title_match.is_none() && !tag_match {
                all_match = false;
                break;
            }
            if title_match.is_some() {
                score += 3;
            }
            if tag_match {
                score += 2;
            }
            if let Some(idx) = body_match {
                score += 1;
                first_idx = Some(first_idx.map_or(idx, |existing| existing.min(idx)));
            }
        }
        if !all_match {
            continue;
        }

        let snippet = first_idx
            .map(|idx| make_snippet(&text, idx))
            .unwrap_or_else(|| make_snippet(&text, 0));

        hits.push(SearchHit {
            path: visible_rel,
            title,
            snippet,
            score,
        });
    }

    hits.sort_by(|a, b| b.score.cmp(&a.score).then_with(|| a.path.cmp(&b.path)));
    hits.truncate(MAX_HITS);
    Ok(hits)
}

fn make_snippet(src: &str, around: usize) -> String {
    let len = src.len();
    let start_byte = around.saturating_sub(SNIPPET_CHARS / 2);
    let end_byte = (start_byte + SNIPPET_CHARS).min(len);
    // Walk to char boundaries.
    let mut s = start_byte;
    while s > 0 && !src.is_char_boundary(s) {
        s -= 1;
    }
    let mut e = end_byte;
    while e < len && !src.is_char_boundary(e) {
        e += 1;
    }
    let slice = &src[s..e];
    let cleaned = clean_markup(&slice.replace('\n', " "));
    let trimmed = cleaned.trim();
    if s > 0 {
        format!("…{trimmed}")
    } else {
        trimmed.to_string()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::space::test_helpers::make_space_with_note;

    #[test]
    fn empty_query_returns_nothing() {
        let (_dir, space, pass) = make_space_with_note("p", "a.md", "# A\n\nbody");
        assert!(search(&space, &pass, "").unwrap().is_empty());
        assert!(search(&space, &pass, "   ").unwrap().is_empty());
    }

    #[test]
    fn finds_body_match() {
        let (_dir, space, pass) =
            make_space_with_note("p", "a.md", "# Sunday\n\nThe quick brown fox");
        let hits = search(&space, &pass, "brown fox").unwrap();
        assert_eq!(hits.len(), 1);
        assert_eq!(hits[0].path, "a.md");
        assert!(hits[0].snippet.contains("brown fox"));
    }

    #[test]
    fn title_hit_scores_higher_than_body_hit() {
        let (_dir, space, pass) =
            make_space_with_note("p", "title.md", "# Memory palaces\n\nshort");
        crate::space::write::write_file(
            &space,
            &pass,
            "body.md",
            "# Other\n\nThe word memory appears here.",
            None,
        )
        .unwrap();
        let hits = search(&space, &pass, "memory").unwrap();
        assert_eq!(hits.len(), 2);
        // The title hit should rank first.
        assert_eq!(hits[0].path, "title.md");
    }

    #[test]
    fn and_semantics_require_all_tokens_to_match() {
        let (_dir, space, pass) = make_space_with_note(
            "p",
            "a.md",
            "# Note\n\nThe quick brown fox jumps over the lazy dog.",
        );
        crate::space::write::write_file(&space, &pass, "b.md", "# Just brown.\n\nshort", None)
            .unwrap();
        let hits = search(&space, &pass, "brown fox").unwrap();
        assert_eq!(hits.len(), 1);
        assert_eq!(hits[0].path, "a.md");
    }

    #[test]
    fn case_insensitive_match() {
        let (_dir, space, pass) = make_space_with_note("p", "a.md", "# T\n\nHELLO world");
        let hits = search(&space, &pass, "hello").unwrap();
        assert_eq!(hits.len(), 1);
    }

    #[test]
    fn no_match_returns_empty() {
        let (_dir, space, pass) = make_space_with_note("p", "a.md", "# T\n\nnothing");
        assert!(search(&space, &pass, "xyz123").unwrap().is_empty());
    }

    #[test]
    fn snippet_strips_markdown_emphasis() {
        let (_dir, space, pass) =
            make_space_with_note("p", "a.md", "# T\n\nA *star* and **bold** match here.");
        let hits = search(&space, &pass, "match").unwrap();
        assert!(!hits[0].snippet.contains('*'));
    }

    #[test]
    fn matches_against_tags() {
        // Body and title don't contain "garden", but the tag does.
        let (_dir, space, pass) =
            make_space_with_note("p", "a.md", "# Sunday\n\nThe quick brown fox.");
        crate::space::meta::set_tags(&space, &pass, "a.md", vec!["garden".into()]).unwrap();
        let hits = search(&space, &pass, "garden").unwrap();
        assert_eq!(hits.len(), 1);
        assert_eq!(hits[0].path, "a.md");
    }

    #[test]
    fn tag_hit_outranks_body_hit() {
        // Two notes both match "memory" — one as a tag, one in the body. The
        // tag hit should score higher.
        let (_dir, space, pass) = make_space_with_note("p", "tagged.md", "# Tagged\n\nshort body");
        crate::space::meta::set_tags(&space, &pass, "tagged.md", vec!["memory".into()]).unwrap();
        crate::space::write::write_file(
            &space,
            &pass,
            "body.md",
            "# Other\n\nThe word memory appears here.",
            None,
        )
        .unwrap();
        let hits = search(&space, &pass, "memory").unwrap();
        assert_eq!(hits.len(), 2);
        assert_eq!(hits[0].path, "tagged.md");
    }

    #[test]
    fn tag_match_is_case_insensitive() {
        let (_dir, space, pass) = make_space_with_note("p", "a.md", "# Sunday\n\nBody.");
        crate::space::meta::set_tags(&space, &pass, "a.md", vec!["WORK".into()]).unwrap();
        let hits = search(&space, &pass, "work").unwrap();
        assert_eq!(hits.len(), 1);
    }
}
