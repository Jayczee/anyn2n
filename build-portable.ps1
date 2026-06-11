# Portable 打包脚本
$version = "1.0.0"
$portableDir = "dist-portable\anyn2n-v$version-portable\anyn2n"

Write-Host "Building..."
bun run tauri build

if ($LASTEXITCODE -ne 0) {
    Write-Host "Build failed!"
    exit 1
}

Write-Host "Creating portable package..."
if (Test-Path "dist-portable") {
    Remove-Item "dist-portable" -Recurse -Force
}
New-Item -ItemType Directory -Path $portableDir -Force | Out-Null

Copy-Item "src-tauri\target\release\tauri-native.exe" "$portableDir\anyn2n.exe" -Force

$size = "{0:N2} MB" -f ((Get-Item "$portableDir\anyn2n.exe").Length / 1MB)
Write-Host "✓ Portable package: anyn2n.exe ($size)"
Write-Host "  (edge.exe + tap-installer.exe 已嵌入)"
