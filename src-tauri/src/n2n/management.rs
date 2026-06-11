use anyhow::Result;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::net::{SocketAddr, UdpSocket};
use std::process::Command;
use std::time::Duration;

/// 创建不弹 CMD 窗口的子进程（Windows: CREATE_NO_WINDOW）
fn hidden_cmd(name: &str) -> Command {
    let mut c = Command::new(name);
    #[cfg(target_os = "windows")]
    {
        use std::os::windows::process::CommandExt;
        c.creation_flags(0x08000000); // CREATE_NO_WINDOW
    }
    c
}

/// 将日志写入全局 buffer（同步版本）
fn log_to_buffer(level: &str, module: &str, message: String) -> Result<()> {
    let level = level.to_string();
    let module = module.to_string();
    let rt = tokio::runtime::Handle::try_current()?;
    rt.spawn(async move {
        crate::manager::add_global_log(&level, &module, message).await;
    });
    Ok(())
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EdgeStatus {
    pub is_running: bool,
    pub uptime: u64,
    pub tx_packets: u64,
    pub rx_packets: u64,
    pub peers: Vec<PeerInfo>,
    pub supernode_addr: String,
    pub peer_count: u64,
    pub forward_count: u64,
    pub last_super: u64,
    pub last_p2p: u64,
    pub community: String,
    pub local_ip: String,
    pub local_mac: String,
    pub supernode_mac: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PeerInfo {
    pub mac_addr: String,
    pub ip_addr: String,
    pub connection_type: String, // "p2p" or "relay"
    pub last_seen: u64,
}

pub struct EdgeManagementClient {
    socket: UdpSocket,
    management_addr: SocketAddr,
}

impl EdgeManagementClient {
    pub fn new(management_port: u16) -> Result<Self> {
        let socket = UdpSocket::bind("127.0.0.1:0")?;
        socket.set_read_timeout(Some(Duration::from_secs(2)))?;
        let management_addr = format!("127.0.0.1:{}", management_port).parse()?;
        Ok(Self { socket, management_addr })
    }

    fn send_cmd(&self, cmd: &str) -> Result<String> {
        self.socket.send_to(cmd.as_bytes(), self.management_addr)?;
        let mut buf = [0u8; 8192];
        let (len, _) = self.socket.recv_from(&mut buf)?;
        Ok(String::from_utf8_lossy(&buf[..len]).to_string())
    }

    pub fn query_status(&self) -> Result<EdgeStatus> {
        // 先获取 summary (r 命令)
        let summary = self.send_cmd("r\n")?;
        log::debug!("Summary: {}", summary);
        let _ = log_to_buffer("DEBUG", "anyn2n_lib::n2n::management", format!("Summary: {}", summary));

        // 再获取 edges 表格
        let edges_raw = self.send_cmd("edges\n")?;
        log::debug!("Edges: {}", edges_raw);
        let _ = log_to_buffer("DEBUG", "anyn2n_lib::n2n::management", format!("Edges: {}", edges_raw));

        self.parse(&summary, &edges_raw)
    }

    fn parse(&self, summary: &str, edges_raw: &str) -> Result<EdgeStatus> {
        let mut status = EdgeStatus {
            is_running: true,
            uptime: 0,
            tx_packets: 0,
            rx_packets: 0,
            peers: Vec::new(),
            supernode_addr: String::new(),
            peer_count: 0,
            forward_count: 0,
            last_super: 0,
            last_p2p: 0,
            community: String::new(),
            local_ip: String::new(),
            local_mac: String::new(),
            supernode_mac: String::new(),
        };

        // 解析 summary 行: uptime N | pend_peers N | known_peers N | transop N,N
        for line in summary.lines() {
            let line = line.trim();
            if line.starts_with("uptime ") {
                for part in line.split('|') {
                    let kv: Vec<&str> = part.trim().split_whitespace().collect();
                    match kv.first() {
                        Some(&"uptime") => status.uptime = kv.get(1).and_then(|v| v.parse().ok()).unwrap_or(0),
                        Some(&"pend_peers") => {},
                        Some(&"known_peers") => status.peer_count = kv.get(1).and_then(|v| v.parse().ok()).unwrap_or(0),
                        Some(&"transop") => {
                            if let Some(v) = kv.get(1) {
                                let parts: Vec<&str> = v.split(',').collect();
                                status.tx_packets = parts.first().and_then(|v| v.parse().ok()).unwrap_or(0);
                                status.rx_packets = parts.get(1).and_then(|v| v.parse().ok()).unwrap_or(0);
                            }
                        }
                        _ => {}
                    }
                }
            }
            if line.starts_with("super ") {
                if let Some(v) = line.split_whitespace().nth(1) {
                    let parts: Vec<&str> = v.split(',').collect();
                    status.forward_count = parts.first().and_then(|v| v.parse().ok()).unwrap_or(0);
                }
            }
            if line.starts_with("last_super ") {
                status.last_super = line.split_whitespace().nth(1).and_then(|v| v.parse().ok()).unwrap_or(0);
            }
            if line.starts_with("last_p2p ") {
                status.last_p2p = line.split_whitespace().nth(1).and_then(|v| v.parse().ok()).unwrap_or(0);
            }
            if line.starts_with("COMMUNITY ") || line.starts_with("community ") {
                status.community = line.split_whitespace().nth(1).unwrap_or("").trim_matches('\'').to_string();
            }
        }

        // 首先从 edges 表格中尝试找到本地 VPN IP 的网段
        let mut local_vpn_subnet: Option<String> = None;
        for line in edges_raw.lines() {
            let line = line.trim();
            if line.is_empty() { continue; }
            let parts: Vec<&str> = line.split('|').collect();
            if parts.len() >= 2 {
                let ip = parts[1].trim();
                if !ip.is_empty() && is_valid_ip(ip) && is_vpn_ip(ip) {
                    // 提取前三段作为子网前缀 (如 10.88.45.x -> 10.88.45)
                    local_vpn_subnet = Some(extract_subnet_prefix(ip));
                    break;
                }
            }
        }

        // 如果没找到，尝试从 summary 或其他地方获取
        // (目前先用这个逻辑)

        // 获取 ARP 表，只保留与本地 VPN 子网匹配的条目
        let arp_table = if let Some(ref subnet) = local_vpn_subnet {
            log::debug!("Detected VPN subnet: {}.x", subnet);
            let _ = log_to_buffer("DEBUG", "anyn2n_lib::n2n::management", format!("Detected VPN subnet: {}.x", subnet));
            get_arp_table_for_subnet(subnet)
        } else {
            log::warn!("Could not detect VPN subnet, ARP filtering disabled");
            let _ = log_to_buffer("WARN", "anyn2n_lib::n2n::management", "Could not detect VPN subnet, ARP filtering disabled".to_string());
            HashMap::new()
        };

        // 解析 edges 表格
        let mut current_section = "";
        for line in edges_raw.lines() {
            let line = line.trim();
            if line.is_empty() { continue; }

            if line.starts_with("SUPERNODE FORWARD") { current_section = "forward"; continue; }
            if line.starts_with("PEER TO PEER") { current_section = "p2p"; continue; }
            if line.starts_with("SUPERNODES") { current_section = "supernodes"; continue; }
            if line.starts_with("=") || line.starts_with("-") || line.starts_with("#") { continue; }
            // 跳过表头行
            if line.contains("TAP") && line.contains("MAC") && line.contains("EDGE") { continue; }

            let parts: Vec<&str> = line.split('|').collect();

            if current_section == "supernodes" && parts.len() >= 3 {
                let label = parts[0].trim();
                let mac = parts[2].trim();
                let addr = if parts.len() > 3 { parts[3].trim() } else { "" };
                if label.contains('l') || label.contains('*') {
                    if !addr.is_empty() && addr != "0.0.0.0:0" {
                        status.supernode_addr = addr.to_string();
                    }
                    if !mac.is_empty() {
                        status.supernode_mac = mac.to_string();
                    }
                }
                continue;
            }

            if parts.len() < 5 { continue; }
            // TAP 列是 VPN IP，EDGE 列是真实 IP:端口
            let vpn_ip = parts[1].trim();
            let mac = parts[2].trim();
            let edge = parts[3].trim();

            log::debug!("Parsing line: {}", line);
            let _ = log_to_buffer("DEBUG", "anyn2n_lib::n2n::management", format!("Parsing line: {}", line));
            log::debug!("  vpn_ip='{}', mac='{}', edge='{}'", vpn_ip, mac, edge);
            let _ = log_to_buffer("DEBUG", "anyn2n_lib::n2n::management", format!("  vpn_ip='{}', mac='{}', edge='{}'", vpn_ip, mac, edge));

            // 跳过内部条目
            if mac.is_empty() || mac == "MAC" || mac.starts_with("01:80") || mac.starts_with("01:00") { continue; }
            if edge.is_empty() || edge == "0.0.0.0:0" { continue; }

            // 如果 VPN IP 为空，尝试从 ARP 表查找
            let final_vpn_ip = if vpn_ip.is_empty() {
                // 规范化 MAC 地址格式用于查找 (统一为冒号分隔小写)
                let normalized_mac = mac.to_lowercase().replace('-', ":");
                arp_table.get(&normalized_mac).cloned().unwrap_or_default()
            } else {
                vpn_ip.to_string()
            };

            // 如果最终还是没有 VPN IP，跳过该条目
            if final_vpn_ip.is_empty() {
                log::warn!("Skipping peer: mac='{}', no VPN IP found (original='{}', arp_table_size={})",
                           mac, vpn_ip, arp_table.len());
                let _ = log_to_buffer("WARN", "anyn2n_lib::n2n::management",
                    format!("Skipping peer: mac='{}', no VPN IP found (original='{}', arp_table_size={})", mac, vpn_ip, arp_table.len()));
                continue;
            }

            let last_seen: u64 = if parts.len() >= 6 { parts[5].trim().parse().unwrap_or(0) } else { 0 };

            let conn_type = match current_section {
                "p2p" => "p2p",
                _ => "relay",
            };

            log::debug!("  Found peer: ip='{}', mac='{}', type='{}'", final_vpn_ip, mac, conn_type);
            let _ = log_to_buffer("DEBUG", "anyn2n_lib::n2n::management",
                format!("  Found peer: ip='{}', mac='{}', type='{}'", final_vpn_ip, mac, conn_type));

            status.peers.push(PeerInfo {
                mac_addr: mac.to_string(),
                ip_addr: final_vpn_ip,
                connection_type: conn_type.to_string(),
                last_seen,
            });
        }

        Ok(status)
    }

    pub fn send_stop(&self) -> Result<()> {
        self.socket.send_to(b"stop\n", self.management_addr)?;
        Ok(())
    }
}

/// 获取系统 ARP 表，返回 MAC -> IP 映射（仅指定子网）
fn get_arp_table_for_subnet(subnet_prefix: &str) -> HashMap<String, String> {
    let mut map = HashMap::new();

    #[cfg(target_os = "windows")]
    let output = hidden_cmd("arp").args(["-a"]).output();

    #[cfg(not(target_os = "windows"))]
    let output = Command::new("sh")
        .args(["-c", "ip neigh show 2>/dev/null || arp -a"])
        .output();

    if let Ok(out) = output {
        let text = String::from_utf8_lossy(&out.stdout);
        for line in text.lines() {
            if let Some((ip, mac)) = parse_arp_line(line) {
                // 只保留匹配指定子网的地址
                if ip.starts_with(subnet_prefix) {
                    map.insert(mac, ip);
                }
            }
        }
    }

    log::debug!("ARP table (subnet {}): {} entries", subnet_prefix, map.len());
    let _ = log_to_buffer("DEBUG", "anyn2n_lib::n2n::management",
        format!("ARP table (subnet {}): {} entries", subnet_prefix, map.len()));
    for (mac, ip) in &map {
        log::debug!("  {} -> {}", mac, ip);
        let _ = log_to_buffer("DEBUG", "anyn2n_lib::n2n::management", format!("  {} -> {}", mac, ip));
    }

    map
}

/// 提取 IP 地址的子网前缀（前三段）
fn extract_subnet_prefix(ip: &str) -> String {
    let parts: Vec<&str> = ip.split('.').collect();
    if parts.len() >= 3 {
        format!("{}.{}.{}", parts[0], parts[1], parts[2])
    } else {
        ip.to_string()
    }
}

/// 解析 ARP 表的一行，提取 IP 和 MAC 地址
fn parse_arp_line(line: &str) -> Option<(String, String)> {
    let line = line.trim();
    if line.is_empty() { return None; }

    // Windows: "  10.207.146.1          00-ff-2a-12-ad-f5     动态"
    #[cfg(target_os = "windows")]
    {
        let parts: Vec<&str> = line.split_whitespace().collect();
        if parts.len() >= 2 {
            let ip = parts[0];
            let mac = parts[1].to_lowercase().replace('-', ":");
            if is_valid_ip(ip) && is_valid_mac(&mac) {
                return Some((ip.to_string(), mac));
            }
        }
    }

    // Linux: "10.207.146.1 dev n2n0 lladdr 00:ff:2a:12:ad:f5 REACHABLE"
    #[cfg(target_os = "linux")]
    {
        let parts: Vec<&str> = line.split_whitespace().collect();
        if let Some(ip_idx) = parts.iter().position(|&p| is_valid_ip(p)) {
            if let Some(mac_idx) = parts.iter().position(|&p| p == "lladdr").map(|i| i + 1) {
                if mac_idx < parts.len() {
                    let ip = parts[ip_idx];
                    let mac = parts[mac_idx].to_lowercase();
                    if is_valid_mac(&mac) {
                        return Some((ip.to_string(), mac));
                    }
                }
            }
        }
    }

    // macOS: "? (10.207.146.1) at 0:ff:2a:12:ad:f5 on utun2 [ethernet]"
    #[cfg(target_os = "macos")]
    {
        if let Some(ip_start) = line.find('(') {
            if let Some(ip_end) = line.find(')') {
                let ip = &line[ip_start + 1..ip_end];
                if let Some(at_pos) = line.find(" at ") {
                    let after_at = &line[at_pos + 4..];
                    if let Some(mac_end) = after_at.find(' ') {
                        let mac = after_at[..mac_end].to_lowercase().replace('.', ":").replace('-', ":");
                        if is_valid_ip(ip) && is_valid_mac(&mac) {
                            return Some((ip.to_string(), mac));
                        }
                    }
                }
            }
        }
    }

    None
}

/// 检查是否是有效的 IPv4 地址
fn is_valid_ip(s: &str) -> bool {
    s.split('.').count() == 4 && s.split('.').all(|p| p.parse::<u8>().is_ok())
}

/// 检查是否是有效的 MAC 地址（冒号分隔）
fn is_valid_mac(s: &str) -> bool {
    let parts: Vec<&str> = s.split(':').collect();
    parts.len() == 6 && parts.iter().all(|p| p.len() == 2 && p.chars().all(|c| c.is_ascii_hexdigit()))
}

/// 判断 IP 是否属于常见的 VPN 私有网段
fn is_vpn_ip(ip: &str) -> bool {
    let parts: Vec<&str> = ip.split('.').collect();
    if parts.len() != 4 { return false; }

    let first: u8 = parts[0].parse().unwrap_or(0);
    let second: u8 = parts[1].parse().unwrap_or(0);

    // 10.0.0.0/8, 172.16.0.0/12, 192.168.0.0/16
    first == 10 || (first == 172 && (16..=31).contains(&second)) || (first == 192 && second == 168)
}
