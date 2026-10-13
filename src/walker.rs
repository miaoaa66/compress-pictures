//! 输入目录扫描

use crate::codec;
use anyhow::Result;
use std::path::{Path, PathBuf};

/// 一个待处理文件：src 为绝对/相对输入路径，rel 为相对输入根目录的路径
pub struct Job {
    pub src: PathBuf,
    pub rel: PathBuf,
}

pub fn collect(root: &Path) -> Result<Vec<Job>> {
    let mut out = Vec::new();
    for entry in walkdir::WalkDir::new(root)
        .follow_links(false)
        .into_iter()
        .filter_map(|e| e.ok())
    {
        if !entry.file_type().is_file() {
            continue;
        }
        let path = entry.path();
        if !codec::is_supported(path) {
            continue;
        }
        let rel = path.strip_prefix(root).unwrap_or(path).to_path_buf();
        out.push(Job {
            src: path.to_path_buf(),
            rel,
        });
    }
    out.sort_by(|a, b| a.rel.cmp(&b.rel));
    Ok(out)
}
