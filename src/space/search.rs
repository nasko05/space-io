use age::secrecy::SecretString;
use walkdir::WalkDir;

use crate::crypto::age_io;
use crate::error::AppResult;
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
/// This is a deliberate "walk-and-grep" instead of a persistent tantivy
/// index — keeping the index in memory only would mean a startup rebuild
/// for every restart, and persisting it would leak plaintext titles/bodies
/// to disk in violation of the SPEC's "server stores ciphertext only"
/// guarantee. Re-evaluate if the corpus crosses ~10k notes.
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

        let Ok(bytes) = std::fs::read(path) else {
            continue;
        };
        let Ok(plaintext) = age_io::decrypt_bytes(&bytes, passphrase) else {
            continue;
        };
        let Ok(text) = String::from_utf8(plaintext) else {
            continue;
        };
        let lower = text.to_ascii_lowercase();
        let title = extract_title(&text);
        let title_lower = title.as_deref().map(|t| t.to_ascii_lowercase());

        let mut score = 0usize;
        let mut first_idx: Option<usize> = None;
        let mut all_match = true;
        for tok in &tokens {
            let body_match = lower.find(tok);
            let title_match = title_lower.as_deref().and_then(|t| t.find(tok));
            if body_match.is_none() && title_match.is_none() {
                all_match = false;
                break;
            }
            if title_match.is_some() {
                score += 3;
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

fn extract_title(src: &str) -> Option<String> {
    src.lines()
        .find_map(|l| l.strip_prefix("# ").map(|t| t.trim().to_string()))
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
    let cleaned = slice
        .replace('\n', " ")
        .replace(['*', '_', '`'], "")
        .replace("[[", "")
        .replace("]]", "");
    let trimmed = cleaned.trim();
    if s > 0 {
        format!("…{trimmed}")
    } else {
        trimmed.to_string()
    }
}
