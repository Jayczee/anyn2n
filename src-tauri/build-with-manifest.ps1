# 构建并复制 manifest 到 exe 旁边
Write-Host "Building release..."
cargo build --release

if ($LASTEXITCODE -eq 0) {
    Write-Host "Copying manifest to exe directory..."
    Copy-Item "resources\app.manifest" "target\release\anyn2n.exe.manifest" -Force
    Write-Host "Build complete! Executable: target\release\tauri-native.exe"
    Write-Host "Manifest: target\release\anyn2n.exe.manifest"
} else {
    Write-Host "Build failed!"
    exit 1
}
