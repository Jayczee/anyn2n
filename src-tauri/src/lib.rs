mod n2n;
mod manager;
mod commands;
mod servers;

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

/// 从 ICO 字节创建 Image
fn ico_to_image(bytes: &[u8]) -> Image<'static> {
    let img = image::load_from_memory(bytes).expect("failed to parse icon");
    let rgba = img.to_rgba8();
    let (w, h) = rgba.dimensions();
    Image::new_owned(rgba.into_raw(), w, h)
}

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
            // 设置主窗口图标为绿色青蛙
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
