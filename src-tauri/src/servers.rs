use anyhow::Result;
use serde::{Deserialize, Serialize};
use std::fs;
use std::net::{SocketAddr, TcpStream, ToSocketAddrs};
use std::path::PathBuf;
use std::time::{Duration, Instant};
use uuid::Uuid;

/// 服务器条目
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ServerEntry {
    pub id: String,
    pub name: String,
    pub ip: String,
    pub port: u16,
    pub default_group: String,
    pub created_at: u64,
}

/// 持久化存储
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
struct ServerStore {
    servers: Vec<ServerEntry>,
}

pub struct ServerManager {
    file_path: PathBuf,
}

impl ServerManager {
    pub fn new(app_data_dir: PathBuf) -> Self {
        let file_path = app_data_dir.join("servers.json");
        if let Some(parent) = file_path.parent() {
            let _ = fs::create_dir_all(parent);
        }
        Self { file_path }
    }

    fn load_store(&self) -> ServerStore {
        match fs::read_to_string(&self.file_path) {
            Ok(content) => serde_json::from_str(&content).unwrap_or_default(),
            Err(_) => ServerStore::default(),
        }
    }

    fn save_store(&self, store: &ServerStore) -> Result<()> {
        let content = serde_json::to_string_pretty(store)?;
        fs::write(&self.file_path, content)?;
        Ok(())
    }

    /// 保存（新增或更新）
    pub fn save(&self, mut entry: ServerEntry) -> Result<ServerEntry> {
        let mut store = self.load_store();
        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap_or_default()
            .as_secs();

        if entry.id.is_empty() {
            entry.id = Uuid::new_v4().to_string();
            entry.created_at = now;
        } else {
            if let Some(existing) = store.servers.iter().find(|s| s.id == entry.id) {
                entry.created_at = existing.created_at;
            } else {
                entry.created_at = now;
            }
        }

        store.servers.retain(|s| s.id != entry.id);
        store.servers.push(entry.clone());
        store.servers.sort_by_key(|s| std::cmp::Reverse(s.created_at));
        self.save_store(&store)?;
        Ok(entry)
    }

    /// 删除
    pub fn delete(&self, id: &str) -> Result<()> {
        let mut store = self.load_store();
        store.servers.retain(|s| s.id != id);
        self.save_store(&store)?;
        Ok(())
    }

    /// 列出所有（不含 RTT）
    pub fn list(&self) -> Result<Vec<ServerEntry>> {
        Ok(self.load_store().servers)
    }

    /// 测量单个 server 的 TCP RTT（毫秒），超时 2s
    pub fn measure_rtt(ip: &str, port: u16) -> Option<u64> {
        let addr_str = format!("{}:{}", ip, port);
        let addr: SocketAddr = match addr_str.to_socket_addrs().ok()?.next() {
            Some(a) => a,
            None => return None,
        };
        let start = Instant::now();
        match TcpStream::connect_timeout(&addr, Duration::from_secs(2)) {
            Ok(_) => Some(start.elapsed().as_millis() as u64),
            Err(_) => None,
        }
    }
}
