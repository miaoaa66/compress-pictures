# picpress — 离线批量图片压缩 CLI（Windows）

**中文** · [English](README.md)

把图片放进一个目录，一条命令批量压缩，可选导出 **原格式 / WebP / AVIF**，并自动生成日志文件。

- **免环境**：单个 `picpress.exe`，拷到任何 Windows 机器直接运行，不依赖 Node / Python / .NET，只用到系统自带 DLL。
- **体积小**：约 7.4 MB。
- **并发**：默认按 CPU 核心数并发处理，单张失败不会中断整批。
- **肉眼无损优先**：默认保留原分辨率，缩放是可选开关。

支持输入：`jpg/jpeg`、`png`、`webp`、`gif`、`tif/tiff`、`bmp`（递归扫描子目录）。

> 暂不支持 `heic/heif`（iPhone 默认拍照格式）、RAW 及相机专有格式；这类文件会被自动跳过。

## 下载

- Gitee 主仓库 Releases：`https://gitee.com/miaoaa66/compress-pictures/releases`，直接下载 `picpress.exe`。
- 下载到的 exe 就是全部所需：**绿色软件，无需安装**，放到任意目录即可。**双击**会进入交互模式，按提示输入目录和格式；也可以在命令行带参数运行（见下文）。卸载只需删除该文件。
- 运行环境：Windows 10 / 11 64 位。目标机器不需要 Rust、Node、Python、.NET 等任何运行时。

## 快速开始

把要压的图片丢进 `photos` 目录，然后在项目根目录执行（`picpress.exe` 就在当前目录，所以前面必须带 `.\`），输出目录 `out` 没有时会自动创建：
![alt text](./preview/image.png)
在项目根目录执行```picpress.exe```，或者在命令行里执行下面的命令：

```powershell

# WebP（推荐，同画质通常比 JPEG 小 25%~35%）
.\picpress.exe -i .\photos -o .\out -f webp

# AVIF（更小，但编码较慢）
.\picpress.exe -i .\photos -o .\out -f avif

# 保持原格式重压缩（JPEG 重压、PNG 无损优化）
.\picpress.exe -i .\photos -o .\out -f original

# 不指定 -f 时进入交互式选择（回车默认选 webp）
.\picpress.exe -i .\photos -o .\out

# 完全不带参数运行（等同于双击 exe）→ 交互模式，依次询问输入目录、输出目录、格式
.\picpress.exe
```

> `.\` 是 PowerShell 的安全设计：不在 PATH 里的命令必须写相对/绝对路径。若想在任何目录直接敲 `picpress`，把 `picpress.exe` 所在目录加入系统 PATH 即可。

> **在 cmd 里执行**：命令写法一样，可以直接用 `.\picpress.exe`。若输出的中文显示为乱码，先执行一次 `chcp 65001` 切到 UTF-8 代码页即可。

跑完会在日志里看到逐文件明细和汇总：

```
[1/6] synthetic.png  3000x2000  PNG → WebP  5.45 MB → 61.7 KB  +98.9%  436ms
[2/6] 03.jpg  5000x3300  JPEG → WebP  1.73 MB → 1.81 MB  -4.4%  1612ms
[5/6] 02.jpg  7000x4170  JPEG → WebP  4.50 MB → 1.49 MB  +66.8%  2472ms
[6/6] 01.jpg  7000x4667  JPEG → WebP  4.45 MB → 1.30 MB  +70.8%  2535ms
──────── 汇总 ────────
文件: 成功 6 · 失败 0 · 共 6
体积: 20.40 MB → 9.41 MB（+53.9%）
耗时: 2.5 秒
```

日志文件写在当前目录的 `logs\compress-<日期>-<时间>.log`，同时也会打印到屏幕。

日志字段从左到右依次为：序号、文件名、尺寸、`输入格式 → 输出格式`、原体积、压缩后体积、压缩率、耗时。压缩率为正表示体积变小（如 `+92.9%` 即体积节省 92.9%），为负表示压缩后反而变大。

## 参数

| 参数 | 说明 |
| --- | --- |
| `-i, --input <DIR>` | 输入目录，递归扫描子目录；省略则交互式询问 |
| `-o, --output <DIR>` | 输出目录，默认保留输入的相对目录结构；省略则交互式询问 |
| `-f, --format <F>` | 输出格式：`original` / `webp` / `avif`；省略则交互式选择 |
| `--preset <P>` | 质量预设：`high` / `medium`（默认）/ `small` |
| `--quality <1-100>` | 手动指定质量，覆盖预设（AVIF 同样使用 1-100 刻度） |
| `--resize <N>` | 按长边缩放到 N 像素（仅缩小，不放大）；默认不缩放、保留原分辨率 |
| `--jobs <N>` | 并发线程数，默认等于 CPU 核心数 |
| `--keep-exif` | 保留 EXIF（仅 JPEG 输出生效）；默认剥离全部元数据 |
| `--flatten` | 不保留相对目录结构，全部平铺到输出目录 |

预设对应的质量分：

| 预设 | JPEG / WebP | AVIF | PNG 优化级别 |
| --- | --- | --- | --- |
| `high` | 88 | 65 | 3 |
| `medium` | 80 | 58 | 4 |
| `small` | 70 | 50 | 6 |

说明：

- 输出扩展名随格式决定：`webp` → `.webp`，`avif` → `.avif`，`original` 则跟随输入类型（`jpg` / `png` / `webp`）。
- 手机照片的 EXIF 方向会被自动矫正后再编码，画面朝向正确；用 `--keep-exif` 时会把方向标记复位为 1，避免二次旋转。
- 压缩后若反而变大（常见于已被压得很狠的小图），日志会如实记录负的压缩率。

## 常见问题

**压缩后体积反而变大？**
常见于已经被压过的小图（例如从网上下载的图片）。可以降低质量（`--quality 60`），或加 `--resize` 限制长边。

**AVIF 文件打不开？**
AVIF 是较新的格式，部分系统自带查看器不支持。可以：用 Chrome / Edge 等新版本浏览器打开；在微软商店安装 *AV1 Video Extension*；或使用 [果核看图](https://pic.ghxi.com/zh/)、[Bandiview](https://www.bandisoft.com/bandiview/) 等支持 AVIF 的第三方看图工具。

**HEIC 照片没有被处理？**
当前不支持 HEIC（iPhone 默认格式），这类文件会被自动跳过。可把手机相机设为「兼容性最佳」（输出 JPEG），或先用其他工具转成 JPEG 再来压缩。

**cmd 里中文是乱码？**
先执行 `chcp 65001` 切换到 UTF-8 代码页，再运行命令。

**大图处理慢、内存占用高？**
一张 9000×13500 的图解码后内存可达数百 MB。内存吃紧时用 `--jobs 2` 降低并发；只想快速缩小体积可加 `--resize 3000`。

**编译出的 `target/` 目录很大？**
`target/` 只是编译缓存，不是交付产物：它已在 `.gitignore` 中，真正发出去的只有约 7.4 MB 的 `picpress.exe`。缓存只在「编译指纹」变化时才新增一套，而且 cargo 不会自动清理旧产物——只改代码不会累积；但改动 `Cargo.toml` 里的 profile（`opt-level`、`debug`、`lto` 等）、`RUSTFLAGS`、依赖版本，或升级 rustc 之后，旧产物会留在原地，目录可能涨到数 GB。

回收空间：删掉 `target` 目录再重新构建即可：

```powershell
Remove-Item .\target -Recurse -Force
.\build.ps1
```

若 `cargo` 不在 PATH（直接敲 `cargo` 报「无法识别为 cmdlet」），可以用全路径：`& "$env:USERPROFILE\.cargo\bin\cargo.exe" clean`。`build.ps1` 内部已使用该全路径，所以构建本身不受 PATH 影响。

## 实测（`medium` 预设，保留原分辨率）

> 实测环境：Windows 64 位、12 线程 CPU。耗时会受图片内容与磁盘速度影响，仅供量级参考。

| 原图 | 原体积 | 输出 | 体积 | 压缩率 | 测试方 |
| --- | --- | --- | --- | --- | --- |
| forest.jpg 9000×5994 | 14.75 MB | WebP | 4.08 MB | -72.4% | ai |
| city.jpg 9000×13500 | 20.95 MB | WebP | 4.71 MB | -77.5% | ai |
| city.jpg + `--resize 3000` | 20.95 MB | WebP 2000×3000 | 635 KB | -97.0% | ai |
| forest.jpg | 14.75 MB | JPEG（original） | 5.07 MB | -65.6% | ai |
| city.jpg | 20.95 MB | JPEG（original） | 6.26 MB | -70.1% | ai |
| 04.jpg 5000×3300 | 2.53 MB | AVIF | 1.48 MB | -41.6% | ai |
| 10m.jpg | 9.55 MB | jpg | 816 KB | -91.66% | 本人 |
| 10m.jpg | 9.55 MB | WebP | 695 KB | -92.9% | 本人 |
| 10m.jpg | 9.55 MB | AVIF | 337 KB | -96.6% | 本人 |
> 缩放才是降体积的主力：`--resize` 一开，10~20MB 的照片可以直接落到几百 KB，且观感几乎无损。

## 项目结构

| 路径 | 说明 |
| --- | --- |
| `src/main.rs` | CLI 入口：参数解析、交互式选择、汇总输出 |
| `src/walker.rs` | 递归扫描输入目录，过滤出支持的图片 |
| `src/codec.rs` | 编解码核心：解码、缩放、EXIF 方向处理、JPEG/PNG/WebP/AVIF 编码 |
| `src/pipeline.rs` | rayon 并发调度，单张失败不中断整批 |
| `src/logger.rs` | 日志落盘（`logs/compress-<时间戳>.log`） |
| `.cargo/config.toml` | 静态链接配置（`crt-static` / `link-self-contained`） |
| `build.ps1` | 一键构建脚本 |

## 从源码构建（仅构建机需要）

构建机需要：Rust（`x86_64-pc-windows-gnu` 目标）、MinGW-w64 的 gcc（编译 mozjpeg / libwebp 等 C 依赖）、nasm（rav1e 的 x86_64 SIMD 汇编）。
目标机器不需要任何东西。

```powershell
.\build.ps1
# 产物：target\release\picpress.exe
# 并自动复制一份到项目根目录 picpress.exe，供上面的快速开始命令直接使用
```

默认从 `C:\devtools\w64devkit\bin` 找 gcc、`C:\devtools\nasm\Library\bin` 找 nasm，可用环境变量 `MINGW_BIN` / `NASM_BIN` 覆盖。

> 注意：release 必须使用 `opt-level = 3`。`-Oz/-Os` 会触发 mozjpeg 渐进式编码的 Huffman 表错乱（`JERR_HUFF_MISSING_CODE`）并写坏内存，导致进程崩溃。参见 [Cargo.toml](Cargo.toml)。

---

## ⚠️ 仓库说明

本项目**主仓库位于 Gitee**，GitHub 为自动单向同步的只读镜像。

1. 你可以自由 Fork GitHub 上的代码副本，但所有代码提交、Bug 反馈、功能建议、Pull Request，请前往 Gitee 主仓库。
2. GitHub 仓库所有文件由 Gitee 自动同步覆盖，任何在此处的手动修改都会丢失。

👉 Gitee 主仓库地址：`https://gitee.com/miaoaa66/compress-pictures`

---

## 📄 开源协议

本项目基于 [MIT License](./LICENSE) 开源，Copyright (c) 2026 Miaoaa。
