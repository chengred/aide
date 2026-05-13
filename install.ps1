# Aide 一键安装脚本
# 编译并安装到系统 PATH，之后可直接在任意目录输入 aide 使用

Write-Host "Aide - AI Agent CLI Installer" -ForegroundColor Green
Write-Host "================================" -ForegroundColor Green
Write-Host ""

# 确保 cargo 可用
if (-not (Get-Command cargo -ErrorAction SilentlyContinue)) {
    Write-Host "错误: 未找到 cargo。请先安装 Rust: https://rustup.rs" -ForegroundColor Red
    exit 1
}

# 编译并安装到 ~/.cargo/bin
Write-Host "[1/2] 编译 release 版本..." -ForegroundColor Cyan
cargo install --path . --force
if ($LASTEXITCODE -ne 0) {
    Write-Host "编译失败，请检查错误信息。" -ForegroundColor Red
    exit 1
}

# 验证安装
Write-Host "[2/2] 验证安装..." -ForegroundColor Cyan
$binDir = "$env:USERPROFILE\.cargo\bin"
if ((Get-Command aide -ErrorAction SilentlyContinue) -or (Test-Path "$binDir\aide.exe")) {
    Write-Host ""
    Write-Host "安装成功！" -ForegroundColor Green
    Write-Host "  二进制位置: $binDir\aide.exe" -ForegroundColor Gray
    Write-Host ""
    Write-Host "使用方法:" -ForegroundColor White
    Write-Host "  aide              # 启动交互模式" -ForegroundColor Gray
    Write-Host "  aide run ""问题""    # 单次查询" -ForegroundColor Gray
    Write-Host "  aide cfg init     # 初始化配置" -ForegroundColor Gray
    Write-Host "  aide list         # 查看可用模型" -ForegroundColor Gray
} else {
    Write-Host ""
    Write-Host "安装完成，但 aide 不在 PATH 中。" -ForegroundColor Yellow
    Write-Host "请将以下目录添加到 PATH:" -ForegroundColor Yellow
    Write-Host "  $binDir" -ForegroundColor White
    Write-Host ""
    Write-Host "或者直接运行:" -ForegroundColor Yellow
    Write-Host "  $binDir\aide.exe" -ForegroundColor White
}
