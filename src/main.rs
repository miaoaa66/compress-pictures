//! 离线批量图片压缩 CLI（Windows 单文件、免环境）

mod codec;
mod logger;
mod pipeline;
mod walker;

use anyhow::{bail, Result};
use clap::Parser;
use codec::{OutFormat, Params, Preset};
use logger::Logger;
use std::io::{self, Write as _};
use std::path::{Path, PathBuf};
use std::sync::Arc;

#[derive(Parser, Debug)]
#[command(
    name = "picpress",
    version,
    about = "离线批量图片压缩工具：单文件、免环境、支持 JPEG/PNG/WebP/AVIF"
)]
struct Cli {
    /// 输入目录（递归扫描子目录）；省略则交互式询问
    #[arg(short, long)]
    input: Option<PathBuf>,

    /// 输出目录（默认保留输入的相对目录结构）；省略则交互式询问
    #[arg(short, long)]
    output: Option<PathBuf>,

    /// 输出格式；不传时进入交互式选择
    #[arg(short, long, value_enum)]
    format: Option<OutFormat>,

    /// 质量预设：high / medium / small
    #[arg(long, value_enum, default_value_t = Preset::Medium)]
    preset: Preset,

    /// 直接指定质量 1-100，覆盖预设（AVIF 同样使用 1-100）
    #[arg(long)]
    quality: Option<u32>,

    /// 按长边缩放，例如 --resize 2000；默认不缩放，保留原分辨率
    #[arg(long)]
    resize: Option<u32>,

    /// 并发线程数，默认等于 CPU 核心数
    #[arg(long)]
    jobs: Option<usize>,

    /// 保留 EXIF（仅 JPEG 输出生效），默认剥离全部元数据
    #[arg(long)]
    keep_exif: bool,

    /// 不保留相对目录结构，全部平铺到输出目录
    #[arg(long)]
    flatten: bool,
}

fn main() {
    // 双击运行（不带任何参数）时结束后暂停，否则控制台窗口会瞬间关闭
    let interactive = std::env::args().count() <= 1;
    let result = run();
    if let Err(e) = &result {
        eprintln!("错误: {e:#}");
    }
    if interactive {
        wait_for_enter();
    }
    if result.is_err() {
        std::process::exit(1);
    }
}

fn run() -> Result<()> {
    let cli = Cli::parse();

    // 缺少输入/输出目录时交互式询问（双击运行时即走这条路径）
    if cli.input.is_none() || cli.output.is_none() {
        println!("picpress — 离线批量图片压缩（交互模式）");
        println!("直接回车使用括号内的默认值\n");
    }
    let input = match cli.input {
        Some(p) => p,
        None => prompt_dir("输入目录（图片所在目录）", default_input_dir())?,
    };
    let output = match cli.output {
        Some(p) => p,
        None => prompt_dir("输出目录", PathBuf::from("out"))?,
    };

    if !input.is_dir() {
        bail!("输入目录不存在或不是目录: {}", input.display());
    }

    let format = match cli.format {
        Some(f) => f,
        None => prompt_format()?,
    };

    let mut params = Params::from_preset(format, cli.preset, cli.quality)?;
    params.resize_long = cli.resize;
    params.keep_exif = cli.keep_exif;

    std::fs::create_dir_all(&output)
        .map_err(|e| anyhow::anyhow!("无法创建输出目录 {}: {e}", output.display()))?;

    let log = Arc::new(Logger::create(Path::new("logs"))?);

    let jobs = walker::collect(&input)?;
    let threads = match cli.jobs {
        Some(n) if n > 0 => n,
        _ => std::thread::available_parallelism()
            .map(|n| n.get())
            .unwrap_or(4),
    };
    // AVIF 编码器（rav1e）按并发度分摊线程，避免与文件级并发互相超额
    let parallel = threads.min(jobs.len().max(1));
    params.avif_threads = (threads / parallel).max(1);

    log.info(&format!("输入目录: {}", input.display()));
    log.info(&format!("输出目录: {}", output.display()));
    log.info(&format!(
        "输出格式: {} · 预设: {} · 质量: {} · 并发: {} 线程",
        format.label(),
        cli.preset.label(),
        params.quality,
        threads
    ));
    match params.resize_long {
        Some(l) => log.info(&format!("缩放: 长边 {} px", l)),
        None => log.info("缩放: 关闭（保留原分辨率）"),
    }
    log.info(&format!(
        "元数据: {}",
        if cli.keep_exif {
            "保留 EXIF（仅 JPEG 输出生效）"
        } else {
            "剥离（并按 EXIF 方向自动矫正画面）"
        }
    ));
    log.info(&format!("待处理文件: {} 个", jobs.len()));
    log.info(&format!("日志文件: {}", log.path().display()));

    if jobs.is_empty() {
        log.warn("输入目录下没有找到受支持的图片（jpg/jpeg/png/webp/gif/tif/tiff/bmp）");
        return Ok(());
    }

    let summary = pipeline::run(
        &jobs,
        &params,
        &output,
        cli.flatten,
        threads,
        &log,
    )?;

    log.info("──────── 汇总 ────────");
    log.info(&format!(
        "文件: 成功 {} · 失败 {} · 共 {}",
        summary.ok, summary.failed, summary.total
    ));
    log.info(&format!(
        "体积: {} → {}（{}）",
        pipeline::human_size(summary.in_bytes),
        pipeline::human_size(summary.out_bytes),
        change_text(summary.in_bytes, summary.out_bytes)
    ));
    log.info(&format!(
        "耗时: {:.1} 秒",
        summary.elapsed_ms as f64 / 1000.0
    ));
    log.info(&format!("日志文件: {}", log.path().display()));

    if summary.failed > 0 {
        log.warn("存在失败文件，明细见日志");
    }
    Ok(())
}

/// 体积变化描述，例如 “-85.6%”
fn change_text(in_bytes: u64, out_bytes: u64) -> String {
    if in_bytes == 0 {
        return "0.0%".to_string();
    }
    let ratio = 1.0 - out_bytes as f64 / in_bytes as f64;
    format!("{:+.1}%", ratio * 100.0)
}

/// 未指定 --format 时的交互式单选
fn prompt_format() -> Result<OutFormat> {
    println!("未指定输出格式，请选择：");
    println!("  1) original  保持输入格式重压缩（JPEG 重压、PNG 无损优化）");
    println!("  2) webp       WebP 有损，同画质体积通常比 JPEG 小 25%~35%");
    println!("  3) avif       AVIF 有损，同画质体积通常比 JPEG 小 40%~55%，编码较慢");
    print!("请输入 1-3（直接回车默认 2）：");
    io::stdout().flush()?;

    let mut line = String::new();
    io::stdin().read_line(&mut line)?;
    let t = line.trim().to_ascii_lowercase();
    Ok(match t.as_str() {
        "" | "2" | "webp" => OutFormat::Webp,
        "1" | "original" => OutFormat::Original,
        "3" | "avif" => OutFormat::Avif,
        other => bail!("无法识别的选择: {other}（请输入 1、2 或 3）"),
    })
}

/// 交互模式下输入目录的默认值：优先 `photos` 子目录，否则当前目录
fn default_input_dir() -> PathBuf {
    let photos = PathBuf::from("photos");
    if photos.is_dir() {
        photos
    } else {
        PathBuf::from(".")
    }
}

/// 交互式询问一个目录，回车则采用默认值
fn prompt_dir(label: &str, default: PathBuf) -> Result<PathBuf> {
    print!("{label}（回车默认 {}）：", default.display());
    io::stdout().flush()?;

    let mut line = String::new();
    io::stdin().read_line(&mut line)?;
    // 用户从资源管理器拖入路径时可能带引号
    let t = line.trim().trim_matches('"');
    if t.is_empty() {
        return Ok(default);
    }
    Ok(PathBuf::from(t))
}

/// 双击运行结束时暂停，等用户按回车再关闭窗口
fn wait_for_enter() {
    print!("\n按回车键退出…");
    let _ = io::stdout().flush();
    let mut line = String::new();
    let _ = io::stdin().read_line(&mut line);
}
