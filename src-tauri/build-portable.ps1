# Portable 打包脚本
# 使用 cargo tauri build 确保前端正确嵌入 + bundle 信息完整
$version = "1.0.0"
$portableDir = "dist-portable\anyn2n-v$version-portable\anyn2n"

Write-Host "Building with cargo tauri build..."
cargo tauri build

if ($LASTEXITCODE -ne 0) {
    Write-Host "Build failed!"
    exit 1
}

Write-Host "Creating portable directory..."
if (Test-Path "dist-portable") {
    Remove-Item "dist-portable" -Recurse -Force
}
New-Item -ItemType Directory -Path $portableDir -Force | Out-Null

Write-Host "Copying files..."
# 主程序（单文件，edge.exe 已通过 embedded.rs 的 include_bytes! 内嵌在 exe 中）
Copy-Item "target\release\AnyN2N.exe" "$portableDir\anyn2n.exe" -Force

Write-Host ""
Write-Host "✓ Portable package created!"
Write-Host "  $portableDir\anyn2n.exe"
Write-Host ""
Write-Host "内嵌资源："
Write-Host "  - edge.exe (n2n edge, 运行时自动提取到 %TEMP%\AnyN2N\)"
Write-Host "  - tap-installer.exe (TAP 驱动安装器，运行时自动提取)"
