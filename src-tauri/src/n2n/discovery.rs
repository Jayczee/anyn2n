use std::net::UdpSocket;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use tokio::sync::RwLock;

const DISCOVERY_PORT: u16 = 55555;
const DISCOVERY_REQUEST: &str = "N2N_DISCOVER";
const DISCOVERY_REPLY_PREFIX: &str = "N2N_HERE:";

pub struct DiscoveryService {
    local_ip: Arc<RwLock<String>>,
    running: Arc<AtomicBool>,
}

impl DiscoveryService {
    pub fn new(local_ip: Arc<RwLock<String>>) -> Self {
        Self {
            local_ip,
            running: Arc::new(AtomicBool::new(false)),
        }
    }

    /// 启动发现监听器（后台任务）
    pub fn start(&self) {
        if self.running.swap(true, Ordering::SeqCst) {
            return; // 已经在运行
        }

        let running = self.running.clone();
        let local_ip = self.local_ip.clone();

        std::thread::spawn(move || {
            let bind_addr = format!("0.0.0.0:{}", DISCOVERY_PORT);
            let socket = match UdpSocket::bind(&bind_addr) {
                Ok(s) => {
                    let _ = s.set_read_timeout(Some(std::time::Duration::from_secs(1)));
                    log::info!("[Discovery] Listener started on {}", bind_addr);
                    s
                }
                Err(e) => {
                    log::warn!("[Discovery] Failed to bind {}: {}", bind_addr, e);
                    running.store(false, Ordering::SeqCst);
                    return;
                }
            };

            let mut buf = [0u8; 256];
            while running.load(Ordering::SeqCst) {
                match socket.recv_from(&mut buf) {
                    Ok((len, src_addr)) => {
                        let msg = String::from_utf8_lossy(&buf[..len]);
                        let my_ip = local_ip.blocking_read().clone();

                        if msg.trim() == DISCOVERY_REQUEST {
                            // 收到发现请求，回复自己的VPN IP（跳过自己发出的广播）
                            let src_ip = src_addr.ip().to_string();
                            if !my_ip.is_empty() && src_ip != my_ip {
                                let reply = format!("{}{}", DISCOVERY_REPLY_PREFIX, my_ip);
                                if let Err(e) = socket.send_to(reply.as_bytes(), src_addr) {
                                    log::debug!("[Discovery] Failed to send reply to {}: {}", src_addr, e);
                                } else {
                                    log::info!("[Discovery] Replied to {} with IP {}", src_ip, my_ip);
                                }
                            }
                        } else if msg.starts_with(DISCOVERY_REPLY_PREFIX) {
                            // 收到回复：提取对方VPN IP，立即ping触发n2n注册
                            let peer_ip = msg[DISCOVERY_REPLY_PREFIX.len()..].trim().to_string();
                            log::info!("[Discovery] Received reply from {}: peer IP = {}", src_addr, peer_ip);
                            if !peer_ip.is_empty() {
                                // 立即ping对方，触发n2n的REGISTER/ARP解析（单次，给3秒超时）
                                let ip = peer_ip.clone();
                                std::thread::spawn(move || {
                                    #[cfg(target_os = "windows")]
                                    let _ = std::process::Command::new("ping")
                                        .args(["-n", "1", "-w", "3000", &ip])
                                        .output();

                                    #[cfg(not(target_os = "windows"))]
                                    let _ = std::process::Command::new("ping")
                                        .args(["-c", "1", "-W", "3", &ip])
                                        .output();

                                    log::info!("[Discovery] Pinged {} to trigger n2n registration", ip);
                                });
                            }
                        } else {
                            log::debug!("[Discovery] Unknown message from {}: {}", src_addr, msg);
                        }
                    }
                    Err(ref e) if e.kind() == std::io::ErrorKind::WouldBlock || e.kind() == std::io::ErrorKind::TimedOut => {
                        // 超时，继续循环
                    }
                    Err(e) => {
                        log::warn!("[Discovery] Recv error: {} (kind={:?})", e, e.kind());
                        break;
                    }
                }
            }
            log::info!("[Discovery] Listener stopped");
        });
    }

    /// 停止发现监听器
    pub fn stop(&self) {
        self.running.store(false, Ordering::SeqCst);
    }

    /// 发送发现广播到 VPN 子网（触发所有在线节点回复）
    pub fn send_discovery(subnet: &str) {
        let broadcast_addr = format!("{}.255:{}", subnet, DISCOVERY_PORT);
        let bind_addr = "0.0.0.0:0"; // 让OS分配源端口

        match UdpSocket::bind(bind_addr) {
            Ok(socket) => {
                let _ = socket.set_broadcast(true);
                match socket.send_to(DISCOVERY_REQUEST.as_bytes(), &broadcast_addr) {
                    Ok(_) => {
                        log::info!("[Discovery] Broadcast sent to {}", broadcast_addr);
                    }
                    Err(e) => {
                        log::warn!("[Discovery] Failed to send broadcast to {}: {}", broadcast_addr, e);
                    }
                }
            }
            Err(e) => {
                log::warn!("[Discovery] Failed to bind socket for broadcast: {}", e);
            }
        }
    }
}
