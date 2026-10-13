# 在 Windows 构建机上产出单文件 exe（目标机器无需任何环境）
#
# 构建前置：
#   - Rust（stable，x86_64-pc-windows-gnu 目标）
#   - MinGW-w64 的 gcc（用于编译 C 依赖：mozjpeg / libwebp），默认 C:\devtools\w64devkit\bin
#   - nasm（rav1e 的 x86_64 SIMD 汇编），默认 C:\devtools\nasm\Library\bin
# 可用环境变量 MINGW_BIN / NASM_BIN 覆盖上面两个默认路径。

# 不能用 Stop：cargo 的进度信息走 stderr，会被 PowerShell 当成错误记录
$ErrorActionPreference = "Continue"

$mingw = if ($env:MINGW_BIN) { $env:MINGW_BIN } else { "C:\devtools\w64devkit\bin" }
$nasm = if ($env:NASM_BIN) { $env:NASM_BIN } else { "C:\devtools\nasm\Library\bin" }
$cargo = Join-Path $env:USERPROFILE ".cargo\bin\cargo.exe"

if (!(Test-Path $cargo)) { throw "未找到 cargo：$cargo，请先安装 Rust 工具链" }
if (!(Test-Path (Join-Path $nasm "nasm.exe"))) { throw "未找到 nasm：$nasm" }

$env:PATH = "$nasm;$mingw;$env:PATH"
& $cargo build --release
if ($LASTEXITCODE -ne 0) { throw "构建失败" }

$exe = Join-Path $PSScriptRoot "target\release\picpress.exe"

# 同步一份到项目根目录，这样 README 里的 `.\picpress.exe ...` 始终可用
$rootExe = Join-Path $PSScriptRoot "picpress.exe"
Copy-Item $exe $rootExe -Force

Write-Host ""
Write-Host ("构建完成：{0}（{1:N1} MB）" -f $exe, ((Get-Item $exe).Length / 1MB))
Write-Host ("已同步到：{0}" -f $rootExe)
Write-Host "该 exe 可直接拷贝到任意 Windows 机器运行，无需安装任何运行时。"
