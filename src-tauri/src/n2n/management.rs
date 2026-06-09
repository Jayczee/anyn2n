use anyhow::Result;
use serde::{Deserialize, Serialize};
use std::net::{SocketAddr, UdpSocket};
use std::time::Duration;

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

        // 再获取 edges 表格
        let edges_raw = self.send_cmd("edges\n")?;
        log::debug!("Edges: {}", edges_raw);

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

            // 跳过内部条目
            if vpn_ip.is_empty() { continue; }
            if mac.is_empty() || mac == "MAC" || mac.starts_with("01:80") || mac.starts_with("01:00") { continue; }
            if edge.is_empty() || edge == "0.0.0.0:0" { continue; }

            let last_seen: u64 = if parts.len() >= 6 { parts[5].trim().parse().unwrap_or(0) } else { 0 };

            let conn_type = match current_section {
                "p2p" => "p2p",
                _ => "relay",
            };

            status.peers.push(PeerInfo {
                mac_addr: mac.to_string(),
                ip_addr: vpn_ip.to_string(),
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
