use anyhow::Result;
use std::process::{Child, Command};
use std::sync::Arc;
use tokio::sync::RwLock;
#[cfg(target_os = "linux")]
use std::process::Stdio;

/// 创建不弹 CMD 窗口的子进程（Windows: CREATE_NO_WINDOW）
#[cfg(any(windows, target_os = "linux"))]
fn hidden_cmd(name: &str) -> Command {
    let mut c = Command::new(name);
    #[cfg(target_os = "windows")]
    {
        use std::os::windows::process::CommandExt;
        c.creation_flags(0x08000000); // CREATE_NO_WINDOW
    }
    c
}

/// Edge 进程管理器
pub struct EdgeProcessManager {
    child: Arc<RwLock<Option<Child>>>,
    management_port: u16,
    pub active: Arc<RwLock<bool>>,
    pub sn_connected: Arc<RwLock<bool>>,    // 是否与超节点建立连接
    pub tun_ready: Arc<RwLock<bool>>,        // TUN 设备是否就绪
    pub last_sn_contact: Arc<RwLock<u64>>,
    pub started_at: Arc<RwLock<u64>>,
    pub local_ip: Arc<RwLock<String>>,       // edge 分配到的 VPN IP
    pub logs: Arc<RwLock<Vec<String>>>,      // edge 进程输出日志
    pre_ips: Arc<RwLock<Vec<String>>>,       // 启动前的本地 IP 列表（用于 diff）
}

impl EdgeProcessManager {
    pub fn new() -> Self {
        Self {
            child: Arc::new(RwLock::new(None)),
            management_port: 5644,
            active: Arc::new(RwLock::new(false)),
            sn_connected: Arc::new(RwLock::new(false)),
            tun_ready: Arc::new(RwLock::new(false)),
            last_sn_contact: Arc::new(RwLock::new(0)),
            started_at: Arc::new(RwLock::new(0)),
            local_ip: Arc::new(RwLock::new(String::new())),
            logs: Arc::new(RwLock::new(Vec::new())),
            pre_ips: Arc::new(RwLock::new(Vec::new())),
        }
    }

    /// 启动 edge 进程（使用 Tauri sidecar）
    pub async fn start(
        &self,
        community: String,
        supernode: String,
        virtual_ip: Option<String>,
        encryption_key: Option<String>,
    ) -> Result<()> {
        // 停止旧进程
        self.stop().await?;

        // 快照当前所有本地 IP（用于后续 diff 检测 VPN IP）
        *self.pre_ips.write().await = get_all_local_ips();
        log::info!("Snapshotted {} local IPs before edge start", self.pre_ips.read().await.len());

        log::info!("Starting edge process...");

        // 构建命令参数
        let mut args = vec![
            "-v".to_string(),
            "-c".to_string(),
            community.clone(),
            "-l".to_string(),
            supernode.clone(),
            "-t".to_string(),
            self.management_port.to_string(),
        ];

        // 虚拟 IP：手动指定时传 -a static:IP，自动时不传（超节点自动分配）
        if let Some(ip) = virtual_ip {
            args.push("-a".to_string());
            args.push(format!("static:{}", ip));
        }
        // 不传 -a = edge 默认行为 = 超节点自动分配 IP

        // 加密密钥
        if let Some(key) = encryption_key {
            args.push("-k".to_string());
            args.push(key);
        }

        log::info!("Edge args: {:?}", args);

        // Windows
        #[cfg(target_os = "windows")]
        {
            self.start_windows(args).await?;
        }

        // macOS：utun 不需要 root
        #[cfg(target_os = "macos")]
        {
            self.start_macos(args).await?;
        }

        // Linux：TUN 设备需要 root/CAP_NET_ADMIN
        #[cfg(target_os = "linux")]
        {
            self.start_linux(args).await?;
        }

        *self.active.write().await = true;
        *self.sn_connected.write().await = false;
        *self.tun_ready.write().await = false;
        *self.last_sn_contact.write().await = 0;
        *self.local_ip.write().await = String::new();
        *self.started_at.write().await = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap_or_default()
            .as_secs();

        Ok(())
    }

    #[cfg(target_os = "windows")]
    async fn start_windows(&self, args: Vec<String>) -> Result<()> {
        use std::os::windows::process::CommandExt;

        let sidecar_path = crate::embedded::edge_path();

        log::info!("Sidecar path: {:?}", sidecar_path);
        log::info!("Edge args: {:?}", args);

        // 直接启动 edge（应用自身已有管理员权限），CREATE_NO_WINDOW 防止弹出 CMD 窗口
        let mut child = Command::new(&sidecar_path)
            .args(&args)
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .creation_flags(0x08000000) // CREATE_NO_WINDOW
            .spawn()?;

        // 读取 edge 输出，实时跟踪连接状态
        let sn = self.last_sn_contact.clone();
        let tun = self.tun_ready.clone();
        let sn_ok = self.sn_connected.clone();
        let local_ip = self.local_ip.clone();
        let logs = self.logs.clone();
        if let Some(stderr) = child.stderr.take() {
            let (sn, tun, sn_ok, local_ip, logs) = (sn.clone(), tun.clone(), sn_ok.clone(), local_ip.clone(), logs.clone());
            std::thread::spawn(move || { parse_edge_output(stderr, sn, tun, sn_ok, local_ip, logs); });
        }
        if let Some(stdout) = child.stdout.take() {
            std::thread::spawn(move || { parse_edge_output(stdout, sn, tun, sn_ok, local_ip, logs); });
        }

        *self.child.write().await = Some(child);

        Ok(())
    }

    /// 通过管理端口停止 edge 进程
    pub async fn stop_via_management(&self) -> Result<()> {
        use std::net::UdpSocket;
        let mgmt_addr = format!("127.0.0.1:{}", self.management_port);
        let socket = UdpSocket::bind("127.0.0.1:0")?;
        socket.set_write_timeout(Some(std::time::Duration::from_secs(2)))?;
        socket.send_to(b"stop\n", &mgmt_addr)?;
        log::info!("Sent stop command to edge via management port");
        Ok(())
    }

    /// 强制终止所有 edge 进程
    pub async fn kill_all_edges(&self) -> Result<()> {
        #[cfg(target_os = "windows")]
        {
            let _ = hidden_cmd("taskkill")
                .args(&["/F", "/IM", "edge-x86_64-pc-windows-msvc.exe"])
                .output();
            let _ = hidden_cmd("taskkill")
                .args(&["/F", "/IM", "edge.exe"])
                .output();
        }
        #[cfg(not(target_os = "windows"))]
        {
            // pkill on macOS/Linux — best-effort
            let _ = std::process::Command::new("pkill")
                .args(["-f", "edge-"])
                .output();
        }
        Ok(())
    }

    /// macOS：使用 osascript do shell script 管理员权限启动 edge（原生用户名+密码弹窗）
    #[cfg(target_os = "macos")]
    async fn start_macos(&self, args: Vec<String>) -> Result<()> {
        let sidecar_path = resolve_unix_sidecar("edge-aarch64-apple-darwin", "edge-x86_64-apple-darwin")?;

        log::info!("Sidecar path: {}", sidecar_path);

        // 创建临时日志文件（edge 输出重定向到此）
        let log_dir = std::env::temp_dir().join("anyn2n");
        std::fs::create_dir_all(&log_dir)?;
        let log_path = log_dir.join(format!("edge-{}.log", self.management_port));
        let _ = std::fs::remove_file(&log_path);

        let log_file = log_path.to_string_lossy().replace('\'', "'\\''");

        // 构建 shell 命令：后台启动 edge，输出重定向到日志文件
        // 不使用 nohup（在 osascript do shell script 上下文中会报 ioctl 错误）
        let cmd = format!(
            "'{}' {} > '{}' 2>&1 &",
            sidecar_path.replace('\'', "'\\''"),
            args.join(" "),
            log_file
        );

        // do shell script with administrator privileges 弹出标准 macOS 授权对话框
        // 显示用户名+密码（支持 Touch ID），后台 & 使其立即返回
        let script = format!(
            "do shell script \"{}\" with administrator privileges",
            cmd.replace("\"", "\\\"")
        );

        log::info!("Launching edge via osascript admin dialog...");

        let osa_output = Command::new("osascript")
            .args(["-e", &script])
            .output()?;

        if !osa_output.status.success() {
            let err = String::from_utf8_lossy(&osa_output.stderr);
            let out = String::from_utf8_lossy(&osa_output.stdout);
            if err.contains("User canceled") || err.is_empty() && out.is_empty() {
                return Err(anyhow::anyhow!("用户取消了管理员授权"));
            }
            return Err(anyhow::anyhow!("授权失败: {}", err.trim()));
        }

        log::info!("Edge launched with admin privileges, log: {}", log_path.display());

        // 读取日志文件，实时跟踪连接状态
        let sn = self.last_sn_contact.clone();
        let tun = self.tun_ready.clone();
        let sn_ok = self.sn_connected.clone();
        let ip = self.local_ip.clone();
        let log_lines = self.logs.clone();

        std::thread::spawn(move || {
            // 等待日志文件被 edge 创建
            for _ in 0..30 {
                if log_path.exists() { break; }
                std::thread::sleep(std::time::Duration::from_millis(200));
            }
            if let Ok(file) = std::fs::File::open(&log_path) {
                parse_edge_output(file, sn, tun, sn_ok, ip, log_lines);
            }
        });

        // edge 通过 & 后台运行，没有直接子进程句柄
        // 停止通过 stop_via_management() UDP 命令
        *self.child.write().await = None;

        Ok(())
    }

    /// Linux：TUN 设备需要 root 权限，通过 sudo 启动
    #[cfg(target_os = "linux")]
    async fn start_linux(&self, args: Vec<String>) -> Result<()> {
        use std::process::Command;

        let sidecar_path = resolve_unix_sidecar("edge-x86_64-unknown-linux-gnu", "edge-x86_64-unknown-linux-gnu")?;

        log::info!("Sidecar path: {}", sidecar_path);

        let mut sudo_args = vec![sidecar_path];
        sudo_args.extend(args);

        let mut child = Command::new("sudo")
            .args(&sudo_args)
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()?;

        // 读取 edge 输出，实时跟踪连接状态
        spawn_output_readers(&mut child, &self.last_sn_contact, &self.tun_ready, &self.sn_connected, &self.local_ip, &self.logs);

        *self.child.write().await = Some(child);

        Ok(())
    }

    /// 停止 edge 进程
    pub async fn stop(&self) -> Result<()> {
        *self.active.write().await = false;

        // 先尝试通过管理端口优雅停止
        let _ = self.stop_via_management().await;

        // 等待一小会儿
        tokio::time::sleep(tokio::time::Duration::from_millis(500)).await;

        // 强制 kill
        let _ = self.kill_all_edges().await;

        // 清理子进程引用
        if let Some(mut child) = self.child.write().await.take() {
            let _ = child.kill();
            let _ = child.wait();
        }

        *self.local_ip.write().await = String::new();
        log::info!("Edge process stopped");
        Ok(())
    }

    /// 检查是否由本应用启动且正在运行
    pub async fn is_running(&self) -> bool {
        *self.active.read().await
    }

    /// 获取管理端口
    pub fn management_port(&self) -> u16 {
        self.management_port
    }

    /// 检测 VPN IP：对比启动前后的本地 IP，新出现的非回环 IP 即为 VPN IP
    pub async fn detect_vpn_ip(&self) -> Option<String> {
        let current = get_all_local_ips();
        let pre = self.pre_ips.read().await.clone();
        log::info!("Detecting VPN IP: {} pre-IPs, {} current IPs", pre.len(), current.len());
        for ip in &current {
            if !pre.contains(ip)
                && !ip.starts_with("127.")
                && !ip.starts_with("169.254.")
            {
                log::info!("Detected new IP (VPN): {}", ip);
                let mut lip = self.local_ip.write().await;
                *lip = ip.clone();
                return Some(ip.clone());
            }
        }
        log::info!("No new IP detected yet");
        None
    }
}

impl Drop for EdgeProcessManager {
    fn drop(&mut self) {
        if let Some(mut child) = self.child.blocking_write().take() {
            let _ = child.kill();
        }
    }
}

impl Default for EdgeProcessManager {
    fn default() -> Self {
        Self::new()
    }
}

/// 解析 Unix（macOS/Linux）sidecar 路径
#[cfg(not(target_os = "windows"))]
fn resolve_unix_sidecar(aarch64_name: &str, x86_64_name: &str) -> Result<String> {
    if cfg!(debug_assertions) {
        let binary_name = if cfg!(target_arch = "aarch64") {
            aarch64_name
        } else {
            x86_64_name
        };
        Ok(std::env::current_dir()?
            .join("binaries")
            .join(binary_name)
            .to_string_lossy()
            .to_string())
    } else {
        Ok("edge".to_string())
    }
}

/// 为子进程启动 stdout/stderr 读取线程
#[cfg(not(target_os = "windows"))]
fn spawn_output_readers(
    child: &mut std::process::Child,
    last_sn: &Arc<RwLock<u64>>,
    tun_ready: &Arc<RwLock<bool>>,
    sn_connected: &Arc<RwLock<bool>>,
    local_ip: &Arc<RwLock<String>>,
    logs: &Arc<RwLock<Vec<String>>>,
) {
    let sn = last_sn.clone();
    let tun = tun_ready.clone();
    let sn_ok = sn_connected.clone();
    let ip = local_ip.clone();
    let log_lines = logs.clone();

    if let Some(stderr) = child.stderr.take() {
        let (sn, tun, sn_ok, ip, log_lines) = (sn.clone(), tun.clone(), sn_ok.clone(), ip.clone(), log_lines.clone());
        std::thread::spawn(move || {
            parse_edge_output(stderr, sn, tun, sn_ok, ip, log_lines);
        });
    }
    if let Some(stdout) = child.stdout.take() {
        std::thread::spawn(move || {
            parse_edge_output(stdout, sn, tun, sn_ok, ip, log_lines);
        });
    }
}

/// 解析 edge 进程输出，实时更新连接状态
fn parse_edge_output(
    reader: impl std::io::Read + Send + 'static,
    last_sn: Arc<RwLock<u64>>,
    tun_ready: Arc<RwLock<bool>>,
    sn_connected: Arc<RwLock<bool>>,
    local_ip: Arc<RwLock<String>>,
    logs: Arc<RwLock<Vec<String>>>,
) {
    use std::io::BufRead;
    use std::time::{SystemTime, UNIX_EPOCH};
    let buf = std::io::BufReader::new(reader);
    for line in buf.lines() {
        if let Ok(line) = line {
            // 过滤掉高频的数据包传输日志
            if line.contains("Tx PACKET") || line.contains("Rx PACKET") {
                continue;
            }

            // 打印到 dev 控制台
            log::info!("[edge] {}", line);

            // 存入日志 buffer，带完整格式（时间戳 + 模块 + 级别 + 内容）
            if logs.blocking_read().len() < 2000 {
                let timestamp = chrono::Local::now().format("[%Y-%m-%d][%H:%M:%S]");
                let formatted = format!("{}[anyn2n_lib::n2n::edge_process][INFO] [edge] {}", timestamp, line);
                logs.blocking_write().push(formatted);
            }

            let now = || SystemTime::now().duration_since(UNIX_EPOCH).map(|t| t.as_secs()).unwrap_or(0);

            // TUN 设备就绪 + 提取分配的 IP
            // 格式: "created local tap device IP: 10.207.146.48, Mask: ..."
            if line.contains("created local tap device") || line.contains("Interface is up") {
                *tun_ready.blocking_write() = true;
                if let Some(ip) = extract_ip_after_colon(&line, "IP:") {
                    *local_ip.blocking_write() = ip;
                    log::info!("Detected VPN IP from edge output: {}", line);
                }
            }
            // 超节点连接成功
            if line.contains(">>> supernode") || line.contains("[OK] edge") {
                *sn_connected.blocking_write() = true;
                *last_sn.blocking_write() = now();
            }
            // 收到超节点 ACK/PONG
            if line.contains("REGISTER_SUPER_ACK") || line.contains("Rx PONG") {
                *sn_connected.blocking_write() = true;
                *last_sn.blocking_write() = now();
            }
            // 超节点无响应 → 标记断联
            if line.contains("supernode not responding") {
                *sn_connected.blocking_write() = false;
            }
            // Peer P2P 通信也说明网络通路
            if line.contains("[p2p]") {
                *last_sn.blocking_write() = now();
            }
        }
    }
}

/// 从字符串中提取冒号后面紧跟的 IP 地址
/// 例如 "IP: 10.207.146.48" → Some("10.207.146.48")
fn extract_ip_after_colon(line: &str, prefix: &str) -> Option<String> {
    let pos = line.find(prefix)?;
    let after = line[pos + prefix.len()..].trim();
    // after 现在以 IP 开头，可能跟着 ", Mask..." 或空格
    let ip_end = after.find(|c: char| !c.is_ascii_digit() && c != '.')
        .unwrap_or(after.len());
    let ip = after[..ip_end].trim();
    if ip.split('.').count() == 4 && ip.chars().all(|c| c.is_ascii_digit() || c == '.') {
        Some(ip.to_string())
    } else {
        None
    }
}

/// 获取本机所有 IPv4 地址列表
#[cfg(target_os = "windows")]
fn get_all_local_ips() -> Vec<String> {
    let output = hidden_cmd("powershell")
        .args([
            "-NoProfile",
            "-Command",
            "Get-NetIPAddress -AddressFamily IPv4 | Select-Object -ExpandProperty IPAddress",
        ])
        .output();
    match output {
        Ok(out) => {
            String::from_utf8_lossy(&out.stdout)
                .lines()
                .map(|l| l.trim().to_string())
                .filter(|l| !l.is_empty())
                .collect()
        }
        Err(e) => {
            log::warn!("Failed to get local IPs via PowerShell: {}", e);
            Vec::new()
        }
    }
}

#[cfg(not(target_os = "windows"))]
fn get_all_local_ips() -> Vec<String> {
    use std::process::Command;
    let output = Command::new("sh")
        .args([
            "-c",
            "ip -4 addr show 2>/dev/null | grep -oP 'inet \\K[\\d.]+' || ifconfig 2>/dev/null | grep -oP 'inet \\K[\\d.]+'",
        ])
        .output();
    match output {
        Ok(out) => {
            String::from_utf8_lossy(&out.stdout)
                .lines()
                .map(|l| l.trim().to_string())
                .filter(|l| !l.is_empty() && l != "127.0.0.1")
                .collect()
        }
        Err(e) => {
            log::warn!("Failed to get local IPs: {}", e);
            Vec::new()
        }
    }
}
