mod n2n;
mod manager;
mod commands;
mod servers;
#[cfg(windows)]
mod tap_installer;
mod embedded;

use manager::ConnectionManager;
use servers::ServerManager;
use tauri::{Emitter, Manager};
use tauri::tray::{MouseButton, MouseButtonState, TrayIconBuilder, TrayIconEvent};
use tauri::menu::{MenuBuilder, MenuItemBuilder};
use tauri::image::Image;
use tauri::webview::PageLoadEvent;
use tauri_plugin_opener::OpenerExt;
use tauri_plugin_log::{Target, TargetKind};

// 托盘图标
const ICON_GREEN_BYTES: &[u8] = include_bytes!("../icons/frog_original.ico");
const ICON_RED_BYTES: &[u8] = include_bytes!("../icons/frog_red.ico");

/// 从 ICO 字节创建 Tauri Image
fn ico_to_image(bytes: &[u8]) -> Image<'static> {
    let img = image::load_from_memory(bytes).expect("failed to parse icon");
    let rgba = img.to_rgba8();
    let (w, h) = rgba.dimensions();
    Image::new_owned(rgba.into_raw(), w, h)
}

/// 设置 macOS Dock 图标（仅 macOS 生效）
#[cfg(target_os = "macos")]
fn set_dock_icon(icon_bytes: &'static [u8]) {
    use objc2::{AnyThread, MainThreadMarker};
    use objc2_foundation::NSData;
    use objc2_app_kit::{NSApplication, NSImage};

    let mtm = MainThreadMarker::new().expect("set_dock_icon called off main thread");
    let data = NSData::from_vec(icon_bytes.to_vec());
    let image = unsafe { NSImage::initWithData(NSImage::alloc(), &data) };
    let shared_app = NSApplication::sharedApplication(mtm);
    unsafe { shared_app.setApplicationIconImage(image.as_deref()) };
}

#[cfg(not(target_os = "macos"))]
fn set_dock_icon(_icon_bytes: &'static [u8]) {}

/// macOS 兼容性检测：Apple Silicon 不支持，Intel 检查/安装 tuntap
#[cfg(target_os = "macos")]
fn check_macos_compatibility() -> bool {
    use std::process::Command;

    // 检测是否为 Apple Silicon（原生 arm64 或 x86_64 通过 Rosetta）
    let is_apple_silicon = if cfg!(target_arch = "aarch64") {
        true
    } else if let Ok(out) = Command::new("sysctl")
        .args(["-n", "sysctl.proc_translated"])
        .output()
    {
        String::from_utf8_lossy(&out.stdout).trim() == "1"
    } else {
        false
    };

    if is_apple_silicon {
        // 允许通过环境变量绕过检测用于开发调试
        if std::env::var("ANYN2N_SKIP_ARCH_CHECK").is_ok() {
            log::warn!("Apple Silicon 架构检测被 ANYN2N_SKIP_ARCH_CHECK 绕过");
            return true;
        }
        log::error!("Apple Silicon Mac 不兼容：缺少 tuntap 内核扩展");
        let _ = Command::new("osascript")
            .args([
                "-e",
                "display dialog \"AnyN2N 暂不支持 Apple Silicon (M 系列芯片) Mac。\\n\\n原因：n2n edge 需要 TUN/TAP 内核扩展，\\n而 Apple Silicon 禁止第三方内核扩展。\\n\\n请使用 Intel Mac 运行。\" \
                 with title \"AnyN2N - 不支持的平台\" \
                 buttons {\"退出\"} default button \"退出\" \
                 with icon stop",
            ])
            .output();
        return false;
    }

    // Intel Mac：检查 tuntap 是否已安装
    let has_tuntap = std::path::Path::new("/dev/tap0").exists();

    if has_tuntap {
        log::info!("macOS tuntap kext 已安装");
        return true;
    }

    // tuntap 未安装 → 询问用户是否安装
    log::warn!("未检测到 tuntap 内核扩展");
    let result = Command::new("osascript")
        .args([
            "-e",
            "button returned of (display dialog \
             \"AnyN2N 需要 TUN/TAP 内核扩展来创建虚拟网卡。\\n\\n请手动下载安装 tuntap：\\nhttp://tuntaposx.sourceforge.net/\\n\\n安装后需要重启 Mac 并在「系统偏好设置\\n→ 安全性与隐私」中授权内核扩展。\\n\\n是否打开下载页面？\" \
             with title \"AnyN2N - 缺少驱动\" \
             buttons {\"取消\", \"打开下载页\"} \
             default button \"打开下载页\" \
             with icon caution)",
        ])
        .output();

    if let Ok(out) = result {
        let answer = String::from_utf8_lossy(&out.stdout);
        if answer.contains("打开下载页") {
            let _ = Command::new("open")
                .arg("http://tuntaposx.sourceforge.net/")
                .output();
        }
    }

    log::warn!("tuntap 未安装，应用将继续启动但 edge 连接可能失败");
    // 不阻止启动，让用户有机会稍后安装
    true
}

#[cfg(not(target_os = "macos"))]
fn check_macos_compatibility() -> bool { true }

fn external_navigation_plugin<R: tauri::Runtime>() -> tauri::plugin::TauriPlugin<R> {
    tauri::plugin::Builder::<R>::new("external-navigation")
        .on_navigation(|webview, url| {
            let is_internal_host = matches!(
                url.host_str(),
                Some("localhost") | Some("127.0.0.1") | Some("tauri.localhost") | Some("::1")
            );

            let is_internal = url.scheme() == "tauri" || is_internal_host;

            if is_internal {
                return true;
            }

            let is_external_link = matches!(url.scheme(), "http" | "https" | "mailto" | "tel");

            if is_external_link {
                log::info!("opening external link in system browser: {}", url);
                let _ = webview.opener().open_url(url.as_str(), None::<&str>);
                return false;
            }

            true
        })
        .build()
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    let manager = ConnectionManager::new();

    tauri::Builder::default()
        .plugin(
            tauri_plugin_log::Builder::new()
                .level(log::LevelFilter::Debug)
                .targets([
                    Target::new(TargetKind::Stdout),
                    Target::new(TargetKind::LogDir { file_name: None }),
                    Target::new(TargetKind::Webview),
                ])
                .build(),
        )
        .plugin(tauri_plugin_opener::init())
        .plugin(external_navigation_plugin())
        .setup(|app| {
            // macOS 兼容性检测
            if !check_macos_compatibility() {
                std::process::exit(1);
            }

            // 检查并安装 TAP 驱动（仅 Windows）
            #[cfg(windows)]
            if !tap_installer::check_tap_installed() {
                log::warn!("未检测到 TAP-Windows 虚拟网卡，尝试自动安装...");
                match tap_installer::install_tap_driver() {
                    Ok(()) => {
                        log::info!("TAP 驱动安装成功");
                    }
                    Err(e) => {
                        log::error!("TAP 驱动安装失败: {}", e);
                        tap_installer::show_install_error_dialog(&e.to_string());
                        std::process::exit(1);
                    }
                }
            }

            // 设置主窗口图标 + macOS Dock 图标
            set_dock_icon(ICON_GREEN_BYTES);
            if let Some(w) = app.get_webview_window("main") {
                let _ = w.set_icon(ico_to_image(ICON_GREEN_BYTES));
            }
            let app_data_dir = app
                .path()
                .app_data_dir()
                .expect("failed to resolve app_data_dir");
            app.manage(ServerManager::new(app_data_dir.clone()));
            commands::init_settings_dir(app_data_dir);

            // ── 系统托盘 ──
            let show_hide = MenuItemBuilder::with_id("show_hide", "显示/隐藏").build(app)?;
            let quit = MenuItemBuilder::with_id("quit", "退出").build(app)?;
            let menu = MenuBuilder::new(app).item(&show_hide).separator().item(&quit).build()?;

            let red_icon = ico_to_image(ICON_RED_BYTES);

            let _tray = TrayIconBuilder::with_id("main-tray")
                .icon(red_icon)
                .tooltip("AnyN2N - 未连接")
                .menu(&menu)
                .on_menu_event(|app, event| {
                    match event.id().as_ref() {
                        "show_hide" => {
                            if let Some(w) = app.get_webview_window("main") {
                                if w.is_visible().unwrap_or(false) {
                                    let _ = w.hide();
                                } else {
                                    let _ = w.show();
                                    let _ = w.set_focus();
                                }
                            }
                        }
                        "quit" => {
                            // 退出前清理 edge 进程
                            if let Some(manager) = app.try_state::<ConnectionManager>() {
                                let rt = tokio::runtime::Runtime::new().unwrap();
                                let _ = rt.block_on(manager.disconnect());
                            }
                            app.exit(0);
                        }
                        _ => {}
                    }
                })
                .on_tray_icon_event(|tray, event| {
                    if let TrayIconEvent::Click {
                        button: MouseButton::Left,
                        button_state: MouseButtonState::Up,
                        ..
                    } = event
                    {
                        // 双击或单击托盘图标 → 显示主窗口
                        let app = tray.app_handle();
                        if let Some(w) = app.get_webview_window("main") {
                            let _ = w.show();
                            let _ = w.set_focus();
                        }
                    }
                })
                .build(app)?;

            Ok(())
        })
        .on_window_event(|window, event| {
            if let tauri::WindowEvent::CloseRequested { api, .. } = event {
                if window.label() == "main" {
                    let behavior = commands::get_close_behavior();
                    if behavior == "close" {
                        // 直接关闭 → 发事件给前端弹确认窗
                        let _ = window.emit("confirm-close", ());
                        api.prevent_close();
                    } else {
                        let _ = window.hide();
                        api.prevent_close();
                    }
                }
            }
        })
        .manage(manager)
        .invoke_handler(tauri::generate_handler![
            commands::connect,
            commands::disconnect,
            commands::get_status,
            commands::get_logs,
            commands::list_servers,
            commands::save_server,
            commands::delete_server,
            commands::measure_server_rtt,
            commands::ping_peer,
            commands::open_window,
            commands::set_tray_connected,
            commands::get_tap_stats,
            commands::check_firewall_status,
            commands::add_firewall_rule,
            commands::disable_firewall,
            commands::set_close_behavior,
            commands::get_close_behavior_cmd,
            commands::quit_app,
        ])
        .on_page_load(|webview, payload| {
            if webview.label() == "main" && matches!(payload.event(), PageLoadEvent::Finished) {
                log::info!("main webview finished loading");
                let _ = webview.window().show();
            }
        })
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
