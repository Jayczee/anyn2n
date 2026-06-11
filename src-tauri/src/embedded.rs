use std::path::PathBuf;
use std::sync::OnceLock;

static RUNTIME_DIR: OnceLock<PathBuf> = OnceLock::new();

fn runtime_dir() -> &'static PathBuf {
    RUNTIME_DIR.get_or_init(|| {
        let dir = std::env::temp_dir().join("AnyN2N");
        let _ = std::fs::create_dir_all(&dir);
        dir
    })
}

fn extract(name: &str, bytes: &[u8]) -> PathBuf {
    let path = runtime_dir().join(name);
    if !path.exists() {
        if let Err(e) = std::fs::write(&path, bytes) {
            log::error!("Failed to extract {}: {}", name, e);
        } else {
            log::info!("Extracted {} to {:?}", name, path);
        }
    }
    path
}

pub fn edge_path() -> PathBuf {
    #[cfg(debug_assertions)]
    {
        // Dev: 直接从 binaries/ 加载
        let dev = std::env::current_dir()
            .unwrap_or_default()
            .join("binaries")
            .join("edge-x86_64-pc-windows-msvc.exe");
        if dev.exists() {
            return dev;
        }
    }
    // Release: 从嵌入数据提取
    extract("edge.exe", include_bytes!("../binaries/edge-x86_64-pc-windows-msvc.exe"))
}

pub fn tap_installer_path() -> Option<PathBuf> {
    #[cfg(debug_assertions)]
    {
        let dev = std::env::current_dir()
            .unwrap_or_default()
            .join("binaries")
            .join("tap-installer.exe");
        if dev.exists() {
            return Some(dev);
        }
    }
    // Release: 从嵌入数据提取 (编译时文件必须存在)
    Some(extract("tap-installer.exe", include_bytes!("../binaries/tap-installer.exe")))
}
