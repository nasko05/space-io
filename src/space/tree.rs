use std::path::Path;
use std::time::SystemTime;

use serde::Serialize;
use time::OffsetDateTime;

use crate::error::{AppError, AppResult};
use crate::space::paths::ENC_EXT;
use crate::space::Space;

/// Mirrors the shape used by the UI mock (diary-data.js fileTree).
#[derive(Debug, Serialize)]
#[serde(tag = "type", rename_all = "lowercase")]
pub enum TreeNode {
    Folder {
        name: String,
        path: String,
        children: Vec<TreeNode>,
    },
    File {
        name: String,
        path: String,
        kind: String,
        updated: String,
        size: u64,
    },
}

pub fn build_tree(space: &Space) -> AppResult<Vec<TreeNode>> {
    let root = space.root();
    if !root.is_dir() {
        return Ok(vec![]);
    }
    walk_dir(&root, &root)
}

fn walk_dir(root: &Path, dir: &Path) -> AppResult<Vec<TreeNode>> {
    let mut entries: Vec<_> = std::fs::read_dir(dir)?
        .filter_map(|e| e.ok())
        .filter(|e| {
            let name = e.file_name().to_string_lossy().into_owned();
            !name.starts_with('.')
        })
        .collect();

    entries.sort_by_key(|e| {
        (
            e.file_type().map(|t| !t.is_dir()).unwrap_or(false),
            e.file_name(),
        )
    });

    let mut out = Vec::with_capacity(entries.len());
    for e in entries {
        let path = e.path();
        let ft = e.file_type()?;
        let rel = path
            .strip_prefix(root)
            .map_err(|_| AppError::Internal("path outside root".into()))?;
        let rel_str = rel
            .to_str()
            .ok_or_else(|| AppError::Internal("non-utf8 path".into()))?
            .replace('\\', "/");

        if ft.is_dir() {
            let children = walk_dir(root, &path)?;
            out.push(TreeNode::Folder {
                name: e.file_name().to_string_lossy().into_owned(),
                path: rel_str,
                children,
            });
        } else if ft.is_file() {
            let file_name = e.file_name().to_string_lossy().into_owned();
            let Some(visible_name) = file_name.strip_suffix(ENC_EXT) else {
                continue; // not one of ours
            };
            let visible_rel = rel_str
                .strip_suffix(ENC_EXT)
                .map(str::to_owned)
                .unwrap_or_else(|| rel_str.clone());
            let meta = e.metadata()?;
            let updated = meta
                .modified()
                .ok()
                .and_then(systemtime_iso8601)
                .unwrap_or_default();
            out.push(TreeNode::File {
                name: visible_name.to_string(),
                path: visible_rel,
                kind: classify(visible_name),
                updated,
                size: meta.len(),
            });
        }
    }
    Ok(out)
}

fn classify(name: &str) -> String {
    let ext = name.rsplit('.').next().unwrap_or("").to_ascii_lowercase();
    match ext.as_str() {
        "md" | "markdown" => "md",
        "pdf" => "pdf",
        "docx" | "doc" => "docx",
        "jpg" | "jpeg" | "png" | "gif" | "webp" => "image",
        "mp4" | "mov" | "webm" => "video",
        _ => "file",
    }
    .to_string()
}

fn systemtime_iso8601(t: SystemTime) -> Option<String> {
    let dt: OffsetDateTime = t.into();
    dt.format(&time::format_description::well_known::Rfc3339)
        .ok()
}
