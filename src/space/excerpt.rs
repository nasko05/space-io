use std::collections::BTreeMap;

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

/// Walk every `.md.age` file, decrypt, and return a `path → excerpt` map.
/// Title is the first `# ...` line; excerpt is the first ~180 chars of body.
pub fn build_excerpts(
    space: &Space,
    passphrase: &SecretString,
) -> AppResult<BTreeMap<String, Excerpt>> {
    let root = space.root();
    let mut out = BTreeMap::new();
    if !root.is_dir() {
        return Ok(out);
    }

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

        let title = extract_title(&text);
        let excerpt = extract_excerpt(&text);
        out.insert(visible_rel, Excerpt { title, excerpt });
    }
    Ok(out)
}

fn extract_title(src: &str) -> Option<String> {
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
