//! 图像编解码与元数据处理

use anyhow::{anyhow, bail, Context, Result};
use clap::ValueEnum;
use image::{DynamicImage, GenericImageView};
use std::fs;
use std::path::Path;

#[derive(Debug, Clone, Copy, PartialEq, Eq, ValueEnum)]
pub enum OutFormat {
    /// 保持输入格式重压缩
    Original,
    /// 输出 WebP（有损）
    Webp,
    /// 输出 AVIF（有损）
    Avif,
}

impl OutFormat {
    pub fn label(self) -> &'static str {
        match self {
            OutFormat::Original => "original",
            OutFormat::Webp => "webp",
            OutFormat::Avif => "avif",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, ValueEnum)]
pub enum Preset {
    High,
    Medium,
    Small,
}

impl Preset {
    pub fn label(self) -> &'static str {
        match self {
            Preset::High => "high",
            Preset::Medium => "medium",
            Preset::Small => "small",
        }
    }
}

/// 本次运行的全部压缩参数
pub struct Params {
    pub format: OutFormat,
    /// JPEG / WebP / AVIF 共用的质量分（1-100）
    pub quality: f32,
    /// oxipng 优化级别 0-6
    pub png_level: u8,
    /// AVIF(rav1e) 单文件使用的线程数，由并发度推算
    pub avif_threads: usize,
    /// 按长边缩放，None = 保留原分辨率
    pub resize_long: Option<u32>,
    /// 是否保留 EXIF（仅 JPEG 输出生效）
    pub keep_exif: bool,
}

impl Params {
    pub fn from_preset(format: OutFormat, preset: Preset, override_quality: Option<u32>) -> Result<Self> {
        // AVIF 的画质刻度与 JPEG/WebP 不同，同样“肉眼无损失”需要的数值更低
        let base = match (preset, format) {
            (Preset::High, OutFormat::Avif) => 65.0,
            (Preset::Medium, OutFormat::Avif) => 58.0,
            (Preset::Small, OutFormat::Avif) => 50.0,
            (Preset::High, _) => 88.0,
            (Preset::Medium, _) => 80.0,
            (Preset::Small, _) => 70.0,
        };
        let quality = match override_quality {
            Some(v) => {
                if !(1..=100).contains(&v) {
                    bail!("--quality 需要在 1-100 之间");
                }
                v as f32
            }
            None => base,
        };
        let png_level = match preset {
            Preset::High => 3,
            Preset::Medium => 4,
            Preset::Small => 6,
        };
        Ok(Self {
            format,
            quality,
            png_level,
            avif_threads: 1,
            resize_long: None,
            keep_exif: false,
        })
    }
}

/// 输入文件的格式种类
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Kind {
    Jpeg,
    Png,
    Webp,
    Gif,
    Tiff,
    Bmp,
    Other,
}

pub fn kind_of(path: &Path) -> Kind {
    match path
        .extension()
        .and_then(|s| s.to_str())
        .map(|s| s.to_ascii_lowercase())
        .as_deref()
    {
        Some("jpg") | Some("jpeg") => Kind::Jpeg,
        Some("png") => Kind::Png,
        Some("webp") => Kind::Webp,
        Some("gif") => Kind::Gif,
        Some("tif") | Some("tiff") => Kind::Tiff,
        Some("bmp") => Kind::Bmp,
        _ => Kind::Other,
    }
}

pub fn is_supported(path: &Path) -> bool {
    kind_of(path) != Kind::Other
}

pub fn kind_label(kind: Kind) -> &'static str {
    match kind {
        Kind::Jpeg => "JPEG",
        Kind::Png => "PNG",
        Kind::Webp => "WebP",
        Kind::Gif => "GIF",
        Kind::Tiff => "TIFF",
        Kind::Bmp => "BMP",
        Kind::Other => "未知",
    }
}

/// 输出文件的扩展名（original 模式下随输入类型决定）
pub fn ext_for(kind: Kind, format: OutFormat) -> &'static str {
    match format {
        OutFormat::Webp => "webp",
        OutFormat::Avif => "avif",
        OutFormat::Original => match kind {
            Kind::Jpeg => "jpg",
            Kind::Png => "png",
            Kind::Webp => "webp",
            _ => "png",
        },
    }
}

pub struct Outcome {
    pub out_label: &'static str,
    pub width: u32,
    pub height: u32,
}

pub fn process(src: &Path, dst: &Path, p: &Params) -> Result<Outcome> {
    let raw = fs::read(src).with_context(|| format!("读取失败: {}", src.display()))?;
    let kind = kind_of(src);

    // 情况一：PNG 原格式 + 不缩放 —— 直接做无损优化，避免解码重编码
    if p.format == OutFormat::Original && kind == Kind::Png && p.resize_long.is_none() {
        let out = oxipng::optimize_from_memory(&raw, &png_options(p))
            .map_err(|e| anyhow!("PNG 无损优化失败: {e}"))?;
        let (w, h) = image::image_dimensions(src).unwrap_or((0, 0));
        write_out(dst, &out)?;
        return Ok(Outcome {
            out_label: "PNG 无损",
            width: w,
            height: h,
        });
    }

    // 情况二：解码为像素
    let img = image::load_from_memory(&raw).with_context(|| format!("解码失败: {}", src.display()))?;
    let alpha = img.color().has_alpha();

    // 依据 EXIF 方向矫正画面（手机照片常见），并在保留 EXIF 时把方向标记复位
    let (img, exif) = if kind == Kind::Jpeg {
        let app1 = jpeg_app1(&raw);
        let orientation = app1.map(exif_orientation).unwrap_or(1);
        let img = apply_orientation(img, orientation);
        let exif = if p.keep_exif {
            app1.map(|e| {
                let mut v = e.to_vec();
                if orientation != 1 {
                    set_exif_orientation(&mut v, 1);
                }
                v
            })
        } else {
            None
        };
        (img, exif)
    } else {
        (img, None)
    };

    // 可选的等比缩放
    let img = match p.resize_long {
        Some(long) => resize_long_edge(img, long),
        None => img,
    };

    let (data, out_label) = match p.format {
        OutFormat::Original => match kind {
            Kind::Jpeg => (
                encode_jpeg(&img, p.quality, exif.as_deref())?,
                "JPEG",
            ),
            Kind::Webp => (encode_webp(&img, p.quality, alpha), "WebP"),
            _ => {
                let buf = encode_png(&img)?;
                (reoptimize_png(&buf, p)?, "PNG 无损")
            }
        },
        OutFormat::Webp => (encode_webp(&img, p.quality, alpha), "WebP"),
        OutFormat::Avif => (encode_avif(&img, p.quality, alpha, p.avif_threads)?, "AVIF"),
    };

    write_out(dst, &data)?;
    let (width, height) = img.dimensions();
    Ok(Outcome {
        out_label,
        width,
        height,
    })
}

fn write_out(dst: &Path, data: &[u8]) -> Result<()> {
    if let Some(dir) = dst.parent() {
        fs::create_dir_all(dir).with_context(|| format!("无法创建目录: {}", dir.display()))?;
    }
    fs::write(dst, data).with_context(|| format!("写入失败: {}", dst.display()))?;
    Ok(())
}

fn png_options(p: &Params) -> oxipng::Options {
    let mut o = oxipng::Options::from_preset(p.png_level);
    o.strip = if p.keep_exif {
        oxipng::StripChunks::None
    } else {
        oxipng::StripChunks::Safe
    };
    o
}

fn reoptimize_png(png_bytes: &[u8], p: &Params) -> Result<Vec<u8>> {
    // 无损优化失败时退回未优化的 PNG，仍然可用
    Ok(oxipng::optimize_from_memory(png_bytes, &png_options(p)).unwrap_or_else(|_| png_bytes.to_vec()))
}

fn encode_jpeg(img: &DynamicImage, quality: f32, exif: Option<&[u8]>) -> Result<Vec<u8>> {
    let rgb = img.to_rgb8();
    let (w, h) = (rgb.width() as usize, rgb.height() as usize);

    let mut c = mozjpeg::Compress::new(mozjpeg::ColorSpace::JCS_RGB);
    c.set_size(w, h);
    c.set_quality(quality);
    c.set_optimize_coding(true);
    c.set_progressive_mode();

    let mut started = c.start_compress(Vec::new()).context("启动 JPEG 编码失败")?;
    if let Some(e) = exif {
        // EXIF 段必须在写入扫描线之前写入
        started.write_marker(mozjpeg::Marker::APP(1), e);
    }
    started.write_scanlines(&rgb).context("写入 JPEG 数据失败")?;
    started.finish().context("结束 JPEG 编码失败")
}

fn encode_png(img: &DynamicImage) -> Result<Vec<u8>> {
    let mut buf = Vec::new();
    img.write_to(&mut std::io::Cursor::new(&mut buf), image::ImageFormat::Png)
        .context("PNG 编码失败")?;
    Ok(buf)
}

fn encode_webp(img: &DynamicImage, quality: f32, alpha: bool) -> Vec<u8> {
    if alpha {
        let rgba = img.to_rgba8();
        let enc = webp::Encoder::from_rgba(&rgba, rgba.width(), rgba.height());
        enc.encode(quality).to_vec()
    } else {
        let rgb = img.to_rgb8();
        let enc = webp::Encoder::from_rgb(&rgb, rgb.width(), rgb.height());
        enc.encode(quality).to_vec()
    }
}

fn encode_avif(img: &DynamicImage, quality: f32, alpha: bool, threads: usize) -> Result<Vec<u8>> {
    // AVIF 的并行度：多文件并发运行时每个文件少分几个线程，避免线程超额
    let encoder = ravif::Encoder::new()
        .with_quality(quality)
        .with_speed(6)
        .with_num_threads(Some(threads));

    if alpha {
        let rgba = img.to_rgba8();
        let (w, h) = (rgba.width() as usize, rgba.height() as usize);
        let pixels: Vec<ravif::RGBA8> = rgba
            .pixels()
            .map(|p| ravif::RGBA8::new(p[0], p[1], p[2], p[3]))
            .collect();
        let encoded = encoder
            .encode_rgba(ravif::Img::new(pixels.as_slice(), w, h))
            .map_err(|e| anyhow!("AVIF 编码失败: {e}"))?;
        Ok(encoded.avif_file)
    } else {
        let rgb = img.to_rgb8();
        let (w, h) = (rgb.width() as usize, rgb.height() as usize);
        let pixels: Vec<ravif::RGB8> = rgb
            .pixels()
            .map(|p| ravif::RGB8::new(p[0], p[1], p[2]))
            .collect();
        let encoded = encoder
            .encode_rgb(ravif::Img::new(pixels.as_slice(), w, h))
            .map_err(|e| anyhow!("AVIF 编码失败: {e}"))?;
        Ok(encoded.avif_file)
    }
}

/// 等比缩放到长边 = long（仅缩小，不放大）
fn resize_long_edge(img: DynamicImage, long: u32) -> DynamicImage {
    let (w, h) = img.dimensions();
    if long == 0 || (w <= long && h <= long) {
        return img;
    }
    let scale = long as f64 / w.max(h) as f64;
    let nw = ((w as f64 * scale).round() as u32).max(1);
    let nh = ((h as f64 * scale).round() as u32).max(1);
    img.resize_exact(nw, nh, image::imageops::FilterType::Lanczos3)
}

/// 按 EXIF Orientation 旋转/翻转像素，使画面朝向正确
fn apply_orientation(img: DynamicImage, orientation: u16) -> DynamicImage {
    match orientation {
        2 => img.fliph(),
        3 => img.rotate180(),
        4 => img.flipv(),
        5 => img.rotate90().fliph(),
        6 => img.rotate90(),
        7 => img.rotate270().fliph(),
        8 => img.rotate270(),
        _ => img,
    }
}

/// 取出 JPEG 中的 APP1(Exif) 段载荷（不含 marker 与长度字段）
fn jpeg_app1(data: &[u8]) -> Option<&[u8]> {
    if data.len() < 4 || data[0] != 0xFF || data[1] != 0xD8 {
        return None;
    }
    let mut i = 2usize;
    while i + 4 <= data.len() {
        if data[i] != 0xFF {
            return None;
        }
        let marker = data[i + 1];
        if marker == 0x01 || (0xD0..=0xD9).contains(&marker) {
            i += 2;
            continue;
        }
        if marker == 0xDA {
            // SOS 之后是熵编码数据，元数据段到此为止
            return None;
        }
        let seg_len = ((data[i + 2] as usize) << 8) | data[i + 3] as usize;
        if seg_len < 2 {
            return None;
        }
        let end = i + 2 + seg_len;
        if end > data.len() {
            return None;
        }
        if marker == 0xE1 {
            let payload = &data[i + 4..end];
            if payload.len() >= 6 && &payload[..4] == b"Exif" {
                return Some(payload);
            }
        }
        i = end;
    }
    None
}

fn read_u16(b: &[u8], o: usize, le: bool) -> u16 {
    if o + 2 > b.len() {
        return 0;
    }
    if le {
        u16::from_le_bytes([b[o], b[o + 1]])
    } else {
        u16::from_be_bytes([b[o], b[o + 1]])
    }
}

fn read_u32(b: &[u8], o: usize, le: bool) -> u32 {
    if o + 4 > b.len() {
        return 0;
    }
    if le {
        u32::from_le_bytes([b[o], b[o + 1], b[o + 2], b[o + 3]])
    } else {
        u32::from_be_bytes([b[o], b[o + 1], b[o + 2], b[o + 3]])
    }
}

/// 解析 Exif 载荷头部：返回 (TIFF 区起点偏移, 是否小端, IFD0 相对 TIFF 起点的偏移)
fn exif_header(e: &[u8]) -> Option<(usize, bool, usize)> {
    if e.len() < 14 || &e[..4] != b"Exif" {
        return None;
    }
    let t = &e[6..];
    let le = match &t[..2] {
        b"II" => true,
        b"MM" => false,
        _ => return None,
    };
    if read_u16(t, 2, le) != 0x2A {
        return None;
    }
    let ifd0 = read_u32(t, 4, le) as usize;
    Some((6, le, ifd0))
}

pub fn exif_orientation(e: &[u8]) -> u16 {
    let (base, le, ifd0) = match exif_header(e) {
        Some(v) => v,
        None => return 1,
    };
    let t = &e[base..];
    let count = read_u16(t, ifd0, le) as usize;
    for k in 0..count {
        let entry = ifd0 + 2 + k * 12;
        if entry + 12 > t.len() {
            break;
        }
        if read_u16(t, entry, le) == 0x0112 {
            let v = read_u16(t, entry + 8, le);
            return if v == 0 { 1 } else { v };
        }
    }
    1
}

/// 把 EXIF 的 Orientation 标记改写为指定值（用于已按方向矫正像素后复位为 1）
pub fn set_exif_orientation(e: &mut [u8], value: u16) -> bool {
    let (base, le, ifd0, count) = {
        let (base, le, ifd0) = match exif_header(e) {
            Some(v) => v,
            None => return false,
        };
        let count = read_u16(&e[base..], ifd0, le) as usize;
        (base, le, ifd0, count)
    };
    for k in 0..count {
        let entry = base + ifd0 + 2 + k * 12;
        if entry + 12 > e.len() {
            return false;
        }
        if read_u16(e, entry, le) == 0x0112 {
            let bytes = if le {
                value.to_le_bytes()
            } else {
                value.to_be_bytes()
            };
            e[entry + 8] = bytes[0];
            e[entry + 9] = bytes[1];
            return true;
        }
    }
    false
}
