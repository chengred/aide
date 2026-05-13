# Aide 一键安装脚本
# 优先下载预编译二进制（无需装 Rust），失败则从源码编译

param(
    [switch]$BuildFromSource  # 强制从源码编译
)

$repo = "your-org/aide"
$version = "latest"
$binDir = "$env:USERPROFILE\.cargo\bin"
$binPath = "$binDir\aide.exe"

Write-Host "Aide - AI Agent CLI Installer" -ForegroundColor Green
Write-Host "================================" -ForegroundColor Green
Write-Host ""

# Ensure bin directory exists
New-Item -ItemType Directory -Force -Path $binDir | Out-Null

if (-not $BuildFromSource) {
    Write-Host "下载预编译二进制..." -ForegroundColor Cyan
    try {
        $url = "https://github.com/$repo/releases/$version/download/aide-windows.exe"
        Invoke-WebRequest -Uri $url -OutFile $binPath -ErrorAction Stop
        Write-Host ""
        Write-Host "安装成功！" -ForegroundColor Green
        Write-Host "  二进制位置: $binPath" -ForegroundColor Gray
        ShowUsage
        exit 0
    } catch {
        Write-Host "  下载失败，改为从源码编译..." -ForegroundColor Yellow
    }
}

# Fallback: build from source
if (-not (Get-Command cargo -ErrorAction SilentlyContinue)) {
    Write-Host "错误: 未找到 cargo。请先安装 Rust: https://rustup.rs" -ForegroundColor Red
    exit 1
}

Write-Host "从源码编译..." -ForegroundColor Cyan
cargo install --path . --force
if ($LASTEXITCODE -ne 0) {
    Write-Host "编译失败，请检查错误信息。" -ForegroundColor Red
    exit 1
}

Write-Host ""
Write-Host "安装成功！" -ForegroundColor Green
Write-Host "  二进制位置: $binPath" -ForegroundColor Gray
ShowUsage

function ShowUsage {
    Write-Host ""
    Write-Host "使用方法:" -ForegroundColor White
    Write-Host "  aide              # 启动交互模式" -ForegroundColor Gray
    Write-Host "  aide run ""问题""    # 单次查询" -ForegroundColor Gray
    Write-Host "  aide cfg init     # 初始化配置" -ForegroundColor Gray
    Write-Host "  aide list         # 查看可用模型" -ForegroundColor Gray
}
