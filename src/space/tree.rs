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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::space::test_helpers::make_space;

    fn names(nodes: &[TreeNode]) -> Vec<String> {
        nodes
            .iter()
            .map(|n| match n {
                TreeNode::Folder { name, .. } => name.clone(),
                TreeNode::File { name, .. } => name.clone(),
            })
            .collect()
    }

    #[test]
    fn empty_space_returns_empty_tree() {
        let (_dir, space, _pass) = make_space("p");
        let t = build_tree(&space).unwrap();
        assert!(t.is_empty());
    }

    #[test]
    fn classifies_known_extensions() {
        assert_eq!(classify("a.md"), "md");
        assert_eq!(classify("a.markdown"), "md");
        assert_eq!(classify("doc.pdf"), "pdf");
        assert_eq!(classify("d.docx"), "docx");
        assert_eq!(classify("p.jpg"), "image");
        assert_eq!(classify("p.PNG"), "image");
        assert_eq!(classify("v.mp4"), "video");
        assert_eq!(classify("other.weirdext"), "file");
    }

    #[test]
    fn strips_age_suffix_in_tree() {
        let (dir, space, pass) = make_space("p");
        crate::space::write::write_file(&space, &pass, "Journal/2026/n.md", "x", None).unwrap();
        let t = build_tree(&space).unwrap();
        // root → Journal folder
        let TreeNode::Folder { name, children, .. } = &t[0] else {
            panic!("expected folder")
        };
        assert_eq!(name, "Journal");
        let TreeNode::Folder {
            children: yearly, ..
        } = &children[0]
        else {
            panic!("expected nested folder")
        };
        let TreeNode::File {
            name, path, kind, ..
        } = &yearly[0]
        else {
            panic!("expected file")
        };
        assert_eq!(name, "n.md");
        assert_eq!(path, "Journal/2026/n.md");
        assert_eq!(kind, "md");
        let _ = dir;
    }

    #[test]
    fn ignores_dotfiles() {
        let (dir, space, _pass) = make_space("p");
        let root = dir.path().join("space");
        std::fs::create_dir_all(&root).unwrap();
        std::fs::write(root.join(".gitkeep"), b"").unwrap();
        std::fs::write(root.join(".hidden.md.age"), b"junk").unwrap();
        let t = build_tree(&space).unwrap();
        assert!(t.is_empty(), "dotfiles should be filtered out");
    }

    #[test]
    fn ignores_files_without_age_extension() {
        let (dir, space, _pass) = make_space("p");
        let root = dir.path().join("space");
        std::fs::write(root.join("scratch.md"), b"plaintext leak attempt").unwrap();
        let t = build_tree(&space).unwrap();
        assert!(
            t.is_empty(),
            "plaintext-tail files should not appear in the tree"
        );
    }

    #[test]
    fn folders_appear_before_files() {
        let (dir, space, pass) = make_space("p");
        let root = dir.path().join("space");
        std::fs::create_dir_all(root.join("Beta")).unwrap();
        crate::space::write::write_file(&space, &pass, "alpha.md", "x", None).unwrap();
        let t = build_tree(&space).unwrap();
        let n = names(&t);
        let beta_idx = n.iter().position(|x| x == "Beta").unwrap();
        let alpha_idx = n.iter().position(|x| x == "alpha.md").unwrap();
        assert!(beta_idx < alpha_idx, "got: {n:?}");
    }
}
