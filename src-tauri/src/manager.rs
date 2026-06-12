use crate::n2n::{DiscoveryService, EdgeManagementClient, EdgeProcessManager, EdgeStatus};
use anyhow::Result;
use std::sync::Arc;
use tokio::sync::RwLock;
use std::time::Instant;

/// 全局日志 buffer，供所有模块写入
static GLOBAL_LOGS: once_cell::sync::Lazy<Arc<RwLock<Vec<String>>>> =
    once_cell::sync::Lazy::new(|| Arc::new(RwLock::new(Vec::new())));

/// 添加日志到全局 buffer（供其他模块调用）
pub async fn add_global_log(level: &str, module: &str, message: String) {
    let mut logs = GLOBAL_LOGS.write().await;
    if logs.len() < 5000 {
        let timestamp = chrono::Local::now().format("[%Y-%m-%d][%H:%M:%S]");
        let log_msg = format!("{}[{}][{}] {}", timestamp, module, level, message);
        logs.push(log_msg);
    }
}

/// 全局连接管理器
pub struct ConnectionManager {
    edge_process: Arc<EdgeProcessManager>,
    status: Arc<RwLock<Option<EdgeStatus>>>,
    last_query: Arc<RwLock<Instant>>,
    connect_gen: Arc<RwLock<u64>>, // 连接代际，cancel/reconnect 时递增
    discovery: DiscoveryService,
}

impl ConnectionManager {
    pub fn new() -> Self {
        let edge_process = Arc::new(EdgeProcessManager::new());
        Self {
            discovery: DiscoveryService::new(edge_process.local_ip.clone()),
            edge_process,
            status: Arc::new(RwLock::new(None)),
            last_query: Arc::new(RwLock::new(Instant::now())),
            connect_gen: Arc::new(RwLock::new(0)),
        }
    }

    /// 添加日志
    pub async fn add_log(&self, message: String) {
        log::info!("{}", message);
        add_global_log("INFO", "anyn2n_lib::manager", message).await;
    }

    /// 获取所有日志（合并全局日志 + edge 进程输出）
    pub async fn get_logs(&self) -> Vec<String> {
        let mut all = GLOBAL_LOGS.read().await.clone();
        let edge_logs = self.edge_process.logs.read().await.clone();
        all.extend(edge_logs);
        all
    }

    /// 连接到 Supernode
    pub async fn connect(
        &self,
        community: String,
        supernode: String,
        virtual_ip: Option<String>,
        encryption_key: Option<String>,
    ) -> Result<()> {
        let my_gen = *self.connect_gen.read().await;
        self.add_log(format!("正在连接到 {} ...", supernode)).await;

        // 启动 edge 进程
        self.edge_process
            .start(community.clone(), supernode.clone(), virtual_ip, encryption_key)
            .await?;

        // 检查是否已被取消
        if *self.connect_gen.read().await != my_gen {
            self.add_log("连接已被取消".to_string()).await;
            return Ok(());
        }

        self.add_log("正在启动 edge 进程...（如失败请以管理员身份运行）".to_string()).await;

        // Windows runas 需要较长时间等待 UAC
        #[cfg(target_os = "windows")]
        tokio::time::sleep(tokio::time::Duration::from_secs(5)).await;
        #[cfg(not(target_os = "windows"))]
        tokio::time::sleep(tokio::time::Duration::from_millis(1000)).await;

        // 检查是否已被取消
        if *self.connect_gen.read().await != my_gen {
            self.add_log("连接已被取消".to_string()).await;
            return Ok(());
        }

        // 重试查询状态，最多 5 次
        let mut connected = false;
        for attempt in 1..=5 {
            // 每次重试前检查是否已被取消
            if *self.connect_gen.read().await != my_gen {
                self.add_log("连接已被取消".to_string()).await;
                return Ok(());
            }
            match self.query_status().await {
                Ok(_) => {
                    self.add_log(format!("✓ 已连接 (尝试 {}/{})", attempt, 5)).await;
                    connected = true;
                    break;
                }
                Err(e) => {
                    self.add_log(format!("等待中... ({}/5) {}", attempt, e)).await;
                    tokio::time::sleep(tokio::time::Duration::from_secs(2)).await;
                }
            }
        }

        // 最终检查：代际变了就不报错不杀进程
        if *self.connect_gen.read().await != my_gen {
            self.add_log("连接已被取消".to_string()).await;
            return Ok(());
        }

        if !connected {
            self.add_log("⚠ 连接失败，请检查 supernode 地址和网络".to_string()).await;
            self.edge_process.stop().await?;
            return Err(anyhow::anyhow!(
                "连接失败：无法连接到 supernode，请检查地址和网络"
            ));
        }

        Ok(())
    }

    /// 断开连接
    pub async fn disconnect(&self) -> Result<()> {
        *self.connect_gen.write().await += 1; // 使所有进行中的 connect 失效
        self.discovery.stop();
        self.edge_process.stop().await?;
        *self.status.write().await = None;
        // 清空持久化ARP表，避免旧peer残留
        {
            let mut arp = crate::n2n::management::PERSISTENT_ARP.lock().unwrap();
            arp.mac_to_ip.clear();
            arp.ip_to_mac.clear();
            arp.mac_to_conn_type.clear();
        }
        self.add_log("已断开连接".to_string()).await;
        Ok(())
    }

    /// 查询状态（限流：最少间隔 3 秒）
    pub async fn query_status(&self) -> Result<EdgeStatus> {
        let now = Instant::now();
        {
            let last = *self.last_query.read().await;
            if now.duration_since(last).as_secs() < 3 {
                // 返回缓存
                if let Some(ref s) = *self.status.read().await {
                    let mut cached = s.clone();
                    let (_, sn_connected, sn) = self.get_conn_state().await;
                    if sn > 0 { cached.last_super = sn; cached.is_running = sn <= 45; }
                    let lip = self.edge_process.local_ip.read().await.clone();
                    if !lip.is_empty() {
                        cached.local_ip = lip;
                    } else if sn_connected && cached.local_ip.is_empty() {
                        if let Some(detected) = self.edge_process.detect_vpn_ip().await {
                            cached.local_ip = detected;
                        }
                    }
                    return Ok(cached);
                }
            }
        }
        *self.last_query.write().await = now;

        // 获取 VPN IP（提前获取，供 query_status 内部使用）
        let lip = self.edge_process.local_ip.read().await.clone();

        let client = EdgeManagementClient::new(self.edge_process.management_port())?;
        let mut status = client.query_status(&lip)?;
        if status.last_super > 60 {
            status.is_running = false;
        }
        // 用实时 SN 数据覆盖
        let sn = { let (_, _, s) = self.get_conn_state().await; s };
        if sn > 0 { status.last_super = sn; status.is_running = sn <= 45; }
        if !lip.is_empty() {
            status.local_ip = lip.clone();
        } else {
            let (_, sn_connected, _) = self.get_conn_state().await;
            if sn_connected {
                if let Some(detected) = self.edge_process.detect_vpn_ip().await {
                    status.local_ip = detected.clone();
                }
            }
        }

        // 首次连接成功且检测到VPN子网时，启动子网扫描和定期ping任务
        if !status.local_ip.is_empty() && status.last_super < 45 {
            let parts: Vec<&str> = status.local_ip.rsplitn(2, '.').collect();
            if parts.len() == 2 {
                let subnet = parts[1].to_string();
                self.start_peer_discovery_once(subnet).await;
            }
        }

        *self.status.write().await = Some(status.clone());
        Ok(status)
    }

    /// 首次连接时启动发现服务（只执行一次）
    async fn start_peer_discovery_once(&self, subnet: String) {
        static DISCOVERY_STARTED: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(false);

        if DISCOVERY_STARTED.swap(true, std::sync::atomic::Ordering::SeqCst) {
            return; // 已启动过
        }

        log::info!("[Discovery] Starting discovery service for {}.x", subnet);

        // 启动UDP广播发现监听器（后台线程）
        self.discovery.start();

        // 延迟5秒等edge完成supernode注册，然后发送发现广播
        let subnet_clone = subnet.clone();
        tokio::spawn(async move {
            tokio::time::sleep(tokio::time::Duration::from_secs(5)).await;
            log::info!("[Discovery] Sending initial discovery broadcast");
            DiscoveryService::send_discovery(&subnet_clone);

            // 之后每30秒发送一次发现广播（轻量级，1次广播触达所有peer）
            loop {
                tokio::time::sleep(tokio::time::Duration::from_secs(30)).await;
                DiscoveryService::send_discovery(&subnet_clone);
            }
        });

        // 定期ping持久化表中的已知peer，保持连接活跃
        tokio::spawn(async move {
            loop {
                tokio::time::sleep(tokio::time::Duration::from_secs(15)).await;

                let ips: Vec<String> = {
                    crate::n2n::management::PERSISTENT_ARP.lock().unwrap()
                        .ip_to_mac.keys().cloned().collect()
                };

                if !ips.is_empty() {
                    let mut alive_ips = Vec::new();
                    for ip in &ips {
                        let mut alive = false;
                        #[cfg(target_os = "windows")]
                        {
                            if let Ok(output) = tokio::process::Command::new("ping")
                                .args(["-n", "1", "-w", "100", ip])
                                .output()
                                .await
                            {
                                alive = output.status.success();
                            }
                        }
                        #[cfg(not(target_os = "windows"))]
                        {
                            if let Ok(output) = tokio::process::Command::new("ping")
                                .args(["-c", "1", "-W", "1", ip])
                                .output()
                                .await
                            {
                                alive = output.status.success();
                            }
                        }
                        if alive {
                            alive_ips.push(ip.clone());
                        }
                    }
                    log::info!("[Ping] Keep-alive ping to {} peers, {} responded: {:?}", ips.len(), alive_ips.len(), alive_ips);
                }
            }
        });
    }

    /// 获取当前状态
    pub async fn get_status(&self) -> Option<EdgeStatus> {
        self.status.read().await.clone()
    }

    /// 获取连接状态摘要: (TUN就绪, SN已连接, 上次SN时间)
    pub async fn get_conn_state(&self) -> (bool, bool, u64) {
        let tun = *self.edge_process.tun_ready.read().await;
        let sn = *self.edge_process.sn_connected.read().await;
        let last = *self.edge_process.last_sn_contact.read().await;
        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap_or_default()
            .as_secs();
        if last == 0 { return (tun, false, 999); }
        (tun, sn, now.saturating_sub(last))
    }

    /// 是否正在运行（至少 TUN 已就绪）
    pub async fn is_running(&self) -> bool {
        if !self.edge_process.is_running().await { return false; }
        let started = *self.edge_process.started_at.read().await;
        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap_or_default()
            .as_secs();
        // 启动后 8 秒内是宽限期
        now.saturating_sub(started) < 8 || *self.edge_process.tun_ready.read().await
    }
}

impl Default for ConnectionManager {
    fn default() -> Self {
        Self::new()
    }
}

impl Drop for ConnectionManager {
    fn drop(&mut self) {
        log::info!("App closing, killing edge processes...");
        let edge = self.edge_process.clone();
        // 同步强制 kill
        if let Ok(rt) = tokio::runtime::Runtime::new() {
            rt.block_on(async {
                let _ = edge.stop().await;
            });
        }
    }
}
