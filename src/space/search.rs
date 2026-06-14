use std::sync::Arc;

use age::secrecy::SecretString;
use walkdir::WalkDir;

use crate::crypto::age_io;
use crate::error::AppResult;
use crate::space::excerpt::{clean_markup, extract_title};
use crate::space::paths::visible_markdown_rel;
use crate::space::Space;

const SNIPPET_CHARS: usize = 140;
const MAX_HITS: usize = 24;

pub struct SearchHit {
    pub path: String,
    pub title: Option<String>,
    pub snippet: String,
    pub score: usize,
}

/// Decrypt every `.md.age`, match the query (whitespace-tokenised, AND
/// semantics, case-insensitive), and return up to `MAX_HITS` ordered by score
/// (title 3, tag 2, body 1 per token).
///
/// Walk-and-grep rather than a persistent index: an in-memory index would
/// rebuild on every restart, and persisting one would leak plaintext to disk,
/// breaking the "server stores ciphertext only" guarantee. Plaintext is cached
/// per `(path, mtime)`. Re-evaluate past ~10k notes.
pub fn search(space: &Space, passphrase: &SecretString, query: &str) -> AppResult<Vec<SearchHit>> {
    let tokens: Vec<String> = query
        .split_whitespace()
        .map(|token| token.to_ascii_lowercase())
        .filter(|token| !token.is_empty())
        .collect();
    if tokens.is_empty() {
        return Ok(vec![]);
    }

    let root = space.root();
    if !root.is_dir() {
        return Ok(vec![]);
    }
    let cache = space.cache();

    let meta_index = crate::space::meta::load(space, passphrase)
        .unwrap_or_else(|_| std::sync::Arc::new(crate::space::meta::MetaIndex::default()));

    let mut hits: Vec<SearchHit> = Vec::new();

    for entry in WalkDir::new(&root).into_iter().filter_map(Result::ok) {
        if !entry.file_type().is_file() {
            continue;
        }
        let path = entry.path();
        let Some(visible_rel) = visible_markdown_rel(&root, path) else {
            continue;
        };

        let Some(mtime) = entry.metadata().ok().and_then(|m| m.modified().ok()) else {
            continue;
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
                match cache.get_with_lowered(&cache_key, mtime) {
                    Some(pair) => pair,
                    None => (arc.clone(), Arc::from(arc.to_ascii_lowercase())),
                }
            };

        let title = extract_title(&text);
        let title_lower = title.as_deref().map(|title| title.to_ascii_lowercase());
        let tags_lower: Vec<String> = meta_index
            .paths
            .get(&visible_rel)
            .map(|meta| {
                meta.tags
                    .iter()
                    .map(|tag| tag.to_ascii_lowercase())
                    .collect()
            })
            .unwrap_or_default();

        let mut score = 0usize;
        let mut first_idx: Option<usize> = None;
        let mut all_match = true;
        for token in &tokens {
            let body_match = lower.find(token);
            let title_match = title_lower.as_deref().and_then(|title| title.find(token));
            let tag_match = tags_lower.iter().any(|tag| tag.contains(token));
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
    let mut start = start_byte;
    while start > 0 && !src.is_char_boundary(start) {
        start -= 1;
    }
    let mut end = end_byte;
    while end < len && !src.is_char_boundary(end) {
        end += 1;
    }
    let slice = &src[start..end];
    let cleaned = clean_markup(&slice.replace('\n', " "));
    let trimmed = cleaned.trim();
    if start > 0 {
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
        let (_dir, space, pass) =
            make_space_with_note("p", "a.md", "# Sunday\n\nThe quick brown fox.");
        crate::space::meta::set_tags(&space, &pass, "a.md", vec!["garden".into()]).unwrap();
        let hits = search(&space, &pass, "garden").unwrap();
        assert_eq!(hits.len(), 1);
        assert_eq!(hits[0].path, "a.md");
    }

    #[test]
    fn tag_hit_outranks_body_hit() {
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
