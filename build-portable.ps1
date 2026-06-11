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
Copy-Item "src-tauri\binaries\edge-x86_64-pc-windows-msvc.exe" "$portableDir\edge.exe" -Force
Copy-Item "src-tauri\binaries\wintun.dll" "$portableDir\wintun.dll" -Force

Write-Host ""
Write-Host "✓ Portable package created: $portableDir"
Get-ChildItem $portableDir | ForEach-Object {
    $size = "{0:N2} MB" -f ($_.Length/1MB)
    Write-Host "  - $($_.Name) ($size)"
}
