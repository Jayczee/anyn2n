# Portable 打包脚本
$version = "1.0.0"
$portableDir = "dist-portable\anyn2n-v$version-portable\anyn2n"

Write-Host "Building release..."
cargo build --release

if ($LASTEXITCODE -ne 0) {
    Write-Host "Build failed!"
    exit 1
}

Write-Host "Creating portable directory structure..."
if (Test-Path "dist-portable") {
    Remove-Item "dist-portable" -Recurse -Force
}
New-Item -ItemType Directory -Path $portableDir -Force | Out-Null

Write-Host "Copying files..."
# 主程序
Copy-Item "target\release\AnyN2N.exe" "$portableDir\anyn2n.exe" -Force

# Manifest（UAC 提权）
Copy-Item "resources\app.manifest" "$portableDir\anyn2n.exe.manifest" -Force

# Edge 二进制
Copy-Item "binaries\edge-x86_64-pc-windows-msvc.exe" "$portableDir\edge.exe" -Force

Write-Host ""
Write-Host "✓ Portable package created successfully!"
Write-Host "Location: $portableDir"
Write-Host ""
Write-Host "Files:"
Get-ChildItem $portableDir | ForEach-Object { Write-Host "  - $($_.Name)" }
