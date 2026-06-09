use crate::manager::ConnectionManager;
use crate::n2n::EdgeStatus;
use serde::{Deserialize, Serialize};
use tauri::Manager;

#[derive(Debug, Deserialize)]
pub struct ConnectRequest {
    pub server_address: String,
    pub custom_ip: Option<String>,
    pub community_name: String,
    pub encryption_key: Option<String>,
}

#[derive(Debug, Serialize)]
pub struct StatusResponse {
    pub is_running: bool,
    pub tun_ready: bool,
    pub sn_connected: bool,
    pub status: Option<EdgeStatus>,
}

// ── 结构化错误 ──

#[derive(Debug, Serialize, Clone)]
pub struct ErrorResponse {
    pub error_type: String,  // network | permission | config | process | unknown
    pub message: String,     // 用户友好标题
    pub suggestion: String,  // 操作建议
    pub detail: String,      // 技术细节（写入日志）
}

impl std::fmt::Display for ErrorResponse {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.message)
    }
}

/// 根据 anyhow Error 文本分类错误
fn classify_error(e: impl std::fmt::Display) -> ErrorResponse {
    let msg = e.to_string();
    let lower = msg.to_lowercase();
    if lower.contains("access denied") || lower.contains("permission") || lower.contains("admin") || lower.contains("elevat") {
        ErrorResponse {
            error_type: "permission".into(),
            message: "缺少管理员权限".into(),
            suggestion: "请以管理员身份运行 AnyN2N，edge 需要管理员权限创建虚拟网卡".into(),
            detail: msg,
        }
    } else if lower.contains("timeout") || lower.contains("refused") || lower.contains("unreachable") || lower.contains("10060") || lower.contains("10061") || lower.contains("resolve") {
        ErrorResponse {
            error_type: "network".into(),
            message: "无法连接到 Supernode 服务器".into(),
            suggestion: "请确认服务器地址正确，并检查本机网络和防火墙设置".into(),
            detail: msg,
        }
    } else if lower.contains("invalid") || lower.contains("empty") || lower.contains("format") || lower.contains("config") {
        ErrorResponse {
            error_type: "config".into(),
            message: "配置参数有误".into(),
            suggestion: "请检查 IP 地址格式和必填字段是否完整".into(),
            detail: msg,
        }
    } else if lower.contains("process") || lower.contains("edge") || lower.contains("not found") || lower.contains("sidecar") {
        ErrorResponse {
            error_type: "process".into(),
            message: "Edge 进程异常".into(),
            suggestion: "请确认 edge 二进制文件存在，查看运行日志了解详情".into(),
            detail: msg,
        }
    } else {
        ErrorResponse {
            error_type: "unknown".into(),
            message: "发生未知错误".into(),
            suggestion: "请查看运行日志获取详细信息".into(),
            detail: msg,
        }
    }
}

/// 处理错误：分类 + 序列化为 JSON（不写日志，由前端 toast 展示）
fn handle_error(e: impl std::fmt::Display) -> String {
    let er = classify_error(e);
    serde_json::to_string(&er).unwrap_or_else(|_| er.message.clone())
}

/// 连接到 Supernode
#[tauri::command]
pub async fn connect(
    request: ConnectRequest,
    manager: tauri::State<'_, ConnectionManager>,
) -> Result<String, String> {
    let virtual_ip = if let Some(ip_str) = request.custom_ip {
        if !ip_str.is_empty() { Some(ip_str) } else { None }
    } else {
        None
    };

    manager
        .connect(request.community_name, request.server_address, virtual_ip, request.encryption_key)
        .await
        .map_err(|e| handle_error(e))?;

    Ok("连接成功".to_string())
}

/// 断开连接
#[tauri::command]
pub async fn disconnect(
    manager: tauri::State<'_, ConnectionManager>,
) -> Result<String, String> {
    manager.disconnect().await
        .map_err(|e| handle_error(e))?;
    Ok("已断开连接".to_string())
}

/// 获取连接状态
#[tauri::command]
pub async fn get_status(
    manager: tauri::State<'_, ConnectionManager>,
) -> Result<StatusResponse, String> {
    let is_running = manager.is_running().await;
    let (tun_ready, sn_connected, last_sn) = manager.get_conn_state().await;
    let status = if is_running { manager.query_status().await.ok() } else { None };
    let mut st = status;
    if let Some(ref mut s) = st {
        s.last_super = last_sn.min(999);
        s.is_running = sn_connected || last_sn <= 45;
    }
    Ok(StatusResponse { is_running, tun_ready, sn_connected, status: st })
}

/// 获取日志
#[tauri::command]
pub async fn get_logs(manager: tauri::State<'_, ConnectionManager>) -> Result<Vec<String>, String> {
    Ok(manager.get_logs().await)
}

// ── 服务器列表管理 commands ──

use crate::servers::{ServerEntry, ServerManager};

#[tauri::command]
pub async fn list_servers(srv_mgr: tauri::State<'_, ServerManager>) -> Result<Vec<ServerEntry>, String> {
    srv_mgr.list().map_err(|e| handle_error(e))
}

#[tauri::command]
pub async fn save_server(entry: ServerEntry, srv_mgr: tauri::State<'_, ServerManager>) -> Result<ServerEntry, String> {
    srv_mgr.save(entry).map_err(|e| handle_error(e))
}

#[tauri::command]
pub async fn delete_server(id: String, srv_mgr: tauri::State<'_, ServerManager>) -> Result<(), String> {
    srv_mgr.delete(&id).map_err(|e| handle_error(e))
}

#[tauri::command]
pub async fn measure_server_rtt(ip: String, port: u16) -> Result<Option<u64>, String> {
    Ok(ServerManager::measure_rtt(&ip, port))
}

#[tauri::command]
pub async fn ping_peer(ip: String) -> Result<Option<u64>, String> {
    use std::process::Command;
    #[cfg(target_os = "windows")]
    let output = Command::new("ping").args(["-n", "1", "-w", "2000", &ip]).output();
    #[cfg(not(target_os = "windows"))]
    let output = Command::new("ping").args(["-c", "1", "-W", "2", &ip]).output();

    let out = output.map_err(|e| handle_error(e))?;
    let text = String::from_utf8_lossy(&out.stdout);
    for part in text.split_whitespace() {
        if part.starts_with("time") {
            let num: String = part.chars().filter(|c| c.is_ascii_digit()).collect();
            if let Ok(ms) = num.parse::<u64>() { return Ok(Some(ms)); }
        }
    }
    Ok(None)
}

/// 设置托盘图标状态
#[tauri::command]
pub async fn set_tray_connected(
    connected: bool,
    app: tauri::AppHandle,
) -> Result<(), String> {
    let tray = app.tray_by_id("main-tray").ok_or("tray not found")?;
    if connected {
        let icon = ico_to_image(ICON_GREEN_BYTES);
        tray.set_icon(Some(icon)).map_err(|e| handle_error(e))?;
        tray.set_tooltip(Some("AnyN2N - 已连接")).map_err(|e| handle_error(e))?;
    } else {
        let icon = ico_to_image(ICON_RED_BYTES);
        tray.set_icon(Some(icon)).map_err(|e| handle_error(e))?;
        tray.set_tooltip(Some("AnyN2N - 未连接")).map_err(|e| handle_error(e))?;
    }
    Ok(())
}

fn ico_to_image(bytes: &[u8]) -> tauri::image::Image<'static> {
    let img = image::load_from_memory(bytes).expect("failed to parse icon");
    let rgba = img.to_rgba8();
    let (w, h) = rgba.dimensions();
    tauri::image::Image::new_owned(rgba.into_raw(), w, h)
}

const ICON_GREEN_BYTES: &[u8] = include_bytes!("../../icons/frog_original.ico");
const ICON_RED_BYTES: &[u8] = include_bytes!("../../icons/frog_red.ico");

/// 打开子窗口（peers / logs / servers）
#[tauri::command]
pub async fn open_window(
    app: tauri::AppHandle,
    view: String,
    title: String,
    width: f64,
    height: f64,
) -> Result<(), String> {
    let label = format!("sub_{}", view);
    // 如果窗口已存在，聚焦并返回
    if let Some(w) = app.get_webview_window(&label) {
        let _ = w.set_focus();
        return Ok(());
    }
    use tauri::WebviewWindowBuilder;
    let url = format!("index.html?view={}", view);
    let icon = ico_to_image(ICON_GREEN_BYTES);
    WebviewWindowBuilder::new(&app, &label, tauri::WebviewUrl::App(url.into()))
        .title(title)
        .inner_size(width, height)
        .center()
        .resizable(true)
        .build()
        .inspect(|w| { let _ = w.set_icon(icon); })
        .map_err(|e| handle_error(e))?;
    Ok(())
}

// ── TAP 网卡流量统计 ──

#[derive(Debug, Serialize)]
pub struct TapStats {
    pub rx_bytes: u64,
    pub tx_bytes: u64,
    pub rx_packets: u64,
    pub tx_packets: u64,
}

static TAP_IFACE_NAME: std::sync::OnceLock<std::sync::Mutex<Option<String>>> = std::sync::OnceLock::new();
fn tap_iface_lock() -> &'static std::sync::Mutex<Option<String>> {
    TAP_IFACE_NAME.get_or_init(|| std::sync::Mutex::new(None))
}

fn find_tap_iface() -> Option<String> {
    if let Some(name) = tap_iface_lock().lock().unwrap().as_ref() {
        return Some(name.clone());
    }
    #[cfg(target_os = "windows")]
    {
        for a in ipconfig::get_adapters().unwrap_or_default().iter() {
            let desc = a.description().to_lowercase();
            if desc.contains("tap") || desc.contains("wintun") {
                let name = a.friendly_name().to_string();
                *tap_iface_lock().lock().unwrap() = Some(name.clone());
                return Some(name);
            }
        }
    }
    #[cfg(not(target_os = "windows"))]
    {
        use sysinfo::Networks;
        for (name, _) in Networks::new_with_refreshed_list().iter() {
            let lower = name.to_lowercase();
            if lower.starts_with("tap") || lower.starts_with("tun") || lower.starts_with("n2n") || lower.starts_with("utun") {
                *tap_iface_lock().lock().unwrap() = Some(name.to_string());
                return Some(name.to_string());
            }
        }
    }
    None
}

// 保持一个全局 Networks 实例，每次调用 refresh() 增量获取统计
static NETS: std::sync::OnceLock<std::sync::Mutex<sysinfo::Networks>> = std::sync::OnceLock::new();

fn nets_lock() -> &'static std::sync::Mutex<sysinfo::Networks> {
    NETS.get_or_init(|| std::sync::Mutex::new(sysinfo::Networks::new_with_refreshed_list()))
}

#[tauri::command]
pub async fn get_tap_stats() -> Result<TapStats, String> {
    let iface_name = find_tap_iface();
    let mut nets = nets_lock().lock().unwrap();
    nets.refresh(false); // 增量刷新，保留已有接口，received() 返回差值
    for (name, data) in nets.iter() {
        if iface_name.as_ref().map_or(false, |n| name.contains(n.as_str())) {
            return Ok(TapStats {
                rx_bytes: data.received(),
                tx_bytes: data.transmitted(),
                rx_packets: data.packets_received(),
                tx_packets: data.packets_transmitted(),
            });
        }
    }
    Ok(TapStats { rx_bytes: 0, tx_bytes: 0, rx_packets: 0, tx_packets: 0 })
}

// ── 防火墙管理 ──

#[derive(Debug, Serialize)]
pub struct FirewallStatus {
    pub enabled: bool,
    pub rule_exists: bool,
    pub platform: String,
}

fn edge_path() -> String {
    #[cfg(target_os = "windows")]
    {
        if cfg!(debug_assertions) {
            std::env::current_dir().unwrap_or_default().join("binaries").join("edge-x86_64-pc-windows-msvc.exe").to_string_lossy().to_string()
        } else {
            "edge-x86_64-pc-windows-msvc.exe".to_string()
        }
    }
    #[cfg(not(target_os = "windows"))] { "edge".to_string() }
}

#[tauri::command]
pub async fn check_firewall_status() -> Result<FirewallStatus, String> {
    use std::process::Command;
    #[cfg(any(target_os = "macos", target_os = "linux"))]
    let _prog = edge_path();
    #[cfg(target_os = "windows")]
    {
        let out = Command::new("netsh").args(["advfirewall", "show", "allprofiles", "state"]).output();
        let enabled = out.map_or(false, |o| String::from_utf8_lossy(&o.stdout).contains("ON"));
        let out2 = Command::new("netsh").args(["advfirewall", "firewall", "show", "rule", "name=AnyN2N Edge"]).output();
        let rule_exists = out2.map_or(false, |o| String::from_utf8_lossy(&o.stdout).contains("AnyN2N Edge"));
        Ok(FirewallStatus { enabled, rule_exists, platform: "windows".into() })
    }
    #[cfg(target_os = "macos")]
    {
        let fw = "/usr/libexec/ApplicationFirewall/socketfilterfw";
        let out = Command::new(fw).args(["--getglobalstate"]).output();
        let enabled = out.map_or(false, |o| String::from_utf8_lossy(&o.stdout).contains("on"));
        let out2 = Command::new(fw).args(["--listapps"]).output();
        let rule_exists = out2.map_or(false, |o| String::from_utf8_lossy(&o.stdout).contains(&_prog));
        Ok(FirewallStatus { enabled, rule_exists, platform: "macos".into() })
    }
    #[cfg(target_os = "linux")]
    {
        let enabled = Command::new("firewall-cmd").arg("--state").output().map_or(false, |o| o.status.success())
            || Command::new("ufw").arg("status").output().map_or(false, |o| String::from_utf8_lossy(&o.stdout).contains("active"));
        let rule_exists = false; // Linux 不易可靠检查已有规则
        Ok(FirewallStatus { enabled, rule_exists, platform: "linux".into() })
    }
}

#[tauri::command]
pub async fn add_firewall_rule() -> Result<String, String> {
    use std::process::Command;
    let prog = edge_path();
    #[cfg(target_os = "windows")]
    {
        let out = Command::new("netsh").args(["advfirewall", "firewall", "add", "rule", "name=AnyN2N Edge", "dir=in", "action=allow", &format!("program={}", prog), "enable=yes"]).output().map_err(|e| e.to_string())?;
        if out.status.success() { Ok("已添加防火墙规则，放行 AnyN2N Edge 程序".into()) }
        else { Err(format!("{}", String::from_utf8_lossy(&out.stderr))) }
    }
    #[cfg(target_os = "macos")]
    {
        let fw = "/usr/libexec/ApplicationFirewall/socketfilterfw";
        Command::new(fw).arg("--add").arg(&prog).output().map_err(|e| e.to_string())?;
        Command::new(fw).arg("--unblockapp").arg(&prog).output().map_err(|e| e.to_string())?;
        Ok("已为 AnyN2N Edge 添加 macOS 防火墙例外".into())
    }
    #[cfg(target_os = "linux")]
    {
        let has_fw = Command::new("firewall-cmd").arg("--state").output().map_or(false, |o| o.status.success());
        if has_fw {
            Command::new("sh").args(["-c", "firewall-cmd --permanent --direct --add-rule ipv4 filter INPUT 0 -j ACCEPT -p udp && firewall-cmd --reload"]).output().map_err(|e| e.to_string())?;
        } else {
            let uid = std::process::id().to_string();
            Command::new("sh").args(["-c", &format!("iptables -I INPUT -p udp -j ACCEPT -m comment --comment 'AnyN2N Edge' 2>/dev/null; iptables -I INPUT -p tcp -j ACCEPT -m comment --comment 'AnyN2N Edge' 2>/dev/null")]).output().map_err(|e| e.to_string())?;
        }
        Ok("已添加 Linux 防火墙规则".into())
    }
}

#[tauri::command]
pub async fn disable_firewall() -> Result<String, String> {
    use std::process::Command;
    #[cfg(target_os = "windows")]
    {
        Command::new("netsh").args(["advfirewall", "set", "allprofiles", "state", "off"]).output().map_err(|e| e.to_string())?;
        Ok("已关闭 Windows Defender 防火墙".into())
    }
    #[cfg(target_os = "macos")]
    {
        Command::new("/usr/libexec/ApplicationFirewall/socketfilterfw").args(["--setglobalstate", "off"]).output().map_err(|e| e.to_string())?;
        Ok("已关闭 macOS 防火墙".into())
    }
    #[cfg(target_os = "linux")]
    {
        if Command::new("firewall-cmd").arg("--state").output().map_or(false, |o| o.status.success()) {
            Command::new("sh").args(["-c", "systemctl stop firewalld; systemctl disable firewalld"]).output().map_err(|e| e.to_string())?;
        } else {
            Command::new("sh").args(["-c", "ufw disable"]).output().map_err(|e| e.to_string())?;
        }
        Ok("已关闭 Linux 防火墙".into())
    }
}

// ── 通用设置（持久化到 app_data_dir/anyn2n_settings.json） ──

static SETTINGS_DIR: std::sync::OnceLock<std::path::PathBuf> = std::sync::OnceLock::new();

pub fn init_settings_dir(dir: std::path::PathBuf) {
    let _ = std::fs::create_dir_all(&dir);
    SETTINGS_DIR.set(dir).ok();
}

fn settings_path() -> std::path::PathBuf {
    SETTINGS_DIR.get().cloned().unwrap_or_default().join("settings.json")
}

fn load_settings_map() -> std::collections::HashMap<String, String> {
    std::fs::read_to_string(settings_path())
        .ok()
        .and_then(|s| serde_json::from_str(&s).ok())
        .unwrap_or_default()
}

fn save_settings_map(map: &std::collections::HashMap<String, String>) {
    if let Ok(json) = serde_json::to_string(map) {
        let _ = std::fs::write(settings_path(), json);
    }
}

pub fn get_close_behavior() -> String {
    load_settings_map().get("close_behavior").cloned().unwrap_or_else(|| "minimize".into())
}

#[tauri::command]
pub async fn set_close_behavior(behavior: String) -> Result<(), String> {
    let mut map = load_settings_map();
    map.insert("close_behavior".into(), behavior);
    save_settings_map(&map);
    Ok(())
}

#[tauri::command]
pub async fn get_close_behavior_cmd() -> Result<String, String> {
    Ok(get_close_behavior())
}

#[tauri::command]
pub async fn quit_app(app: tauri::AppHandle) -> Result<(), String> {
    log::info!("quit_app called, exiting...");
    app.exit(0);
    #[allow(unreachable_code)]
    Ok(())
}
