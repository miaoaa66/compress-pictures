//! 批量处理与统计

use crate::codec::{self, Params};
use crate::logger::Logger;
use crate::walker::Job;
use anyhow::{Context, Result};
use rayon::prelude::*;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::Arc;
use std::time::Instant;

pub struct Summary {
    pub total: usize,
    pub ok: usize,
    pub failed: usize,
    pub in_bytes: u64,
    pub out_bytes: u64,
    pub elapsed_ms: u128,
}

struct Record {
    rel: String,
    in_label: &'static str,
    out_label: &'static str,
    in_bytes: u64,
    out_bytes: u64,
    width: u32,
    height: u32,
    elapsed_ms: u128,
}

impl Record {
    fn describe(&self) -> String {
        let ratio = if self.in_bytes == 0 {
            0.0
        } else {
            1.0 - self.out_bytes as f64 / self.in_bytes as f64
        };
        format!(
            "{}  {}x{}  {} → {}  {} → {}  {:+.1}%  {}ms",
            self.rel,
            self.width,
            self.height,
            self.in_label,
            self.out_label,
            human_size(self.in_bytes),
            human_size(self.out_bytes),
            ratio * 100.0,
            self.elapsed_ms
        )
    }
}

pub fn run(
    jobs: &[Job],
    params: &Params,
    output_root: &Path,
    flatten: bool,
    threads: usize,
    log: &Arc<Logger>,
) -> Result<Summary> {
    let start = Instant::now();
    let total = jobs.len();
    let done = AtomicUsize::new(0);

    let pool = rayon::ThreadPoolBuilder::new()
        .num_threads(threads)
        .build()
        .context("创建线程池失败")?;

    let results: Vec<Result<Record>> = pool.install(|| {
        jobs.par_iter()
            .map(|job| {
                let record = process_one(job, params, output_root, flatten);
                let n = done.fetch_add(1, Ordering::Relaxed) + 1;
                match &record {
                    Ok(rec) => log.info(&format!("[{n}/{total}] {}", rec.describe())),
                    Err(e) => log.error(&format!("[{n}/{total}] {} 失败: {e:#}", job.rel.display())),
                }
                record
            })
            .collect()
    });

    let mut summary = Summary {
        total,
        ok: 0,
        failed: 0,
        in_bytes: 0,
        out_bytes: 0,
        elapsed_ms: start.elapsed().as_millis(),
    };
    for r in results {
        match r {
            Ok(rec) => {
                summary.ok += 1;
                summary.in_bytes += rec.in_bytes;
                summary.out_bytes += rec.out_bytes;
            }
            Err(_) => summary.failed += 1,
        }
    }
    Ok(summary)
}

fn process_one(job: &Job, params: &Params, output_root: &Path, flatten: bool) -> Result<Record> {
    let kind = codec::kind_of(&job.src);
    let ext = codec::ext_for(kind, params.format);

    let dst = if flatten {
        let mut p = output_root.join(job.src.file_name().unwrap_or_default());
        p.set_extension(ext);
        p
    } else {
        let mut p = output_root.join(&job.rel);
        p.set_extension(ext);
        p
    };

    let in_bytes = std::fs::metadata(&job.src).map(|m| m.len()).unwrap_or(0);
    let started = Instant::now();

    // 单张图片的任何异常都不应中断整批任务
    let outcome = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        codec::process(&job.src, &dst, params)
    }))
    .map_err(|_| anyhow::anyhow!("处理过程中发生内部异常"))??;

    let out_bytes = std::fs::metadata(&dst).map(|m| m.len()).unwrap_or(0);

    Ok(Record {
        rel: job.rel.display().to_string(),
        in_label: codec::kind_label(kind),
        out_label: outcome.out_label,
        in_bytes,
        out_bytes,
        width: outcome.width,
        height: outcome.height,
        elapsed_ms: started.elapsed().as_millis(),
    })
}

pub fn human_size(bytes: u64) -> String {
    const KB: f64 = 1024.0;
    const MB: f64 = KB * 1024.0;
    let b = bytes as f64;
    if b >= MB {
        format!("{:.2} MB", b / MB)
    } else if b >= KB {
        format!("{:.1} KB", b / KB)
    } else {
        format!("{bytes} B")
    }
}

/// 保留给未来使用的路径拼接工具：确保目录存在
#[allow(dead_code)]
fn ensure_dir(path: &Path) -> Result<PathBuf> {
    if let Some(dir) = path.parent() {
        std::fs::create_dir_all(dir)?;
    }
    Ok(path.to_path_buf())
}
