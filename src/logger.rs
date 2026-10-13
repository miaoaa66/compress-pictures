//! 运行日志：控制台输出 + 落盘到 logs/compress-<时间戳>.log

use anyhow::{Context, Result};
use chrono::Local;
use std::fs::{self, File, OpenOptions};
use std::io::{BufWriter, Write};
use std::path::{Path, PathBuf};
use std::sync::Mutex;

pub struct Logger {
    path: PathBuf,
    file: Mutex<BufWriter<File>>,
}

impl Logger {
    /// 在 dir 下创建 `compress-yyyyMMdd-HHmmss.log`
    pub fn create(dir: &Path) -> Result<Self> {
        fs::create_dir_all(dir).with_context(|| format!("无法创建日志目录: {}", dir.display()))?;
        let name = format!(
            "compress-{}.log",
            Local::now().format("%Y%m%d-%H%M%S")
        );
        let path = dir.join(name);
        let file = OpenOptions::new()
            .create(true)
            .append(true)
            .open(&path)
            .with_context(|| format!("无法创建日志文件: {}", path.display()))?;
        Ok(Self {
            path,
            file: Mutex::new(BufWriter::new(file)),
        })
    }

    pub fn path(&self) -> &Path {
        &self.path
    }

    pub fn info(&self, msg: &str) {
        self.write("INFO", msg);
    }

    pub fn warn(&self, msg: &str) {
        self.write("WARN", msg);
    }

    pub fn error(&self, msg: &str) {
        self.write("ERROR", msg);
    }

    fn write(&self, level: &str, msg: &str) {
        let line = format!(
            "[{}] {:<5} {}",
            Local::now().format("%H:%M:%S"),
            level,
            msg
        );
        println!("{line}");
        if let Ok(mut f) = self.file.lock() {
            // 逐行 flush，保证中途被中断时日志仍然完整
            let _ = writeln!(f, "{line}");
            let _ = f.flush();
        }
    }
}
