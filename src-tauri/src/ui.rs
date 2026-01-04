use serde_json::json;
use std::io;
use tauri::{
    async_runtime::spawn,
    menu::{Menu, MenuItem},
    tray::{MouseButton, MouseButtonState, TrayIconBuilder, TrayIconEvent},
    AppHandle, Emitter, Manager, WindowEvent,
};

use crate::{
    get_autostart_status,
    get_context_menu_status,
    log_event,
    platform::{
        autostart::{register_autostart, unregister_autostart},
        context_menu::{register_context_menu, unregister_context_menu},
    },
};

/// Initialize window behavior and system tray for the application.
/// - Centers and resizes the main window to 80% height of the current monitor.
/// - Hooks close requests to hide the window instead of quitting.
/// - Builds a tray icon with a Quit menu and click-to-toggle visibility.
pub fn init_ui(app: &AppHandle, launch_hidden: bool) -> tauri::Result<()> {
    setup_window(app, launch_hidden)?;
    build_tray(app)?;
    Ok(())
}

fn setup_window(app: &AppHandle, launch_hidden: bool) -> tauri::Result<()> {
    if let Some(window) = app.get_webview_window("main") {
        let window_clone = window.clone();
        window.on_window_event(move |event| {
            if let WindowEvent::CloseRequested { api, .. } = event {
                if window_clone.hide().is_ok() {
                    api.prevent_close();
                }
            }
        });

        let window_clone = window.clone();
        tauri::async_runtime::spawn(async move {
            let _ = window_clone.center();
            if let Ok(Some(monitor)) = window_clone.current_monitor() {
                let monitor_size = monitor.size();
                let window_height = (monitor_size.height as f64 * 0.80) as u32;
                if let Ok(size) = window_clone.outer_size() {
                    let _ = window_clone.set_size(tauri::Size::Physical(tauri::PhysicalSize {
                        width: size.width,
                        height: window_height,
                    }));
                }
                let _ = window_clone.center();
            }
            if launch_hidden {
                let _ = window_clone.hide();
            } else {
                let _ = window_clone.show();
                let _ = window_clone.set_focus();
            }
        });
    }

    Ok(())
}

fn build_tray(app: &AppHandle) -> tauri::Result<()> {
    let quit_item = MenuItem::with_id(app, "quit", "Quit", true, None::<&str>)?;
    let toggle_item = MenuItem::with_id(
        app,
        "toggle_context_menu",
        "Enable Explorer Context Menu",
        true,
        None::<&str>,
    )?;

    let autostart_item = MenuItem::with_id(
        app,
        "toggle_autostart",
        "Enable Autostart with Windows",
        true,
        None::<&str>,
    )?;

    let menu = Menu::with_items(app, &[&toggle_item, &autostart_item, &quit_item])?;

    let icon = match app.default_window_icon() {
        Some(icon) => icon.clone(),
        None => {
            log_event(
                "tray_icon_missing",
                json!({"reason": "default_window_icon returned None", "action": "failing tray init"}),
            );
            return Err(io::Error::new(
                io::ErrorKind::NotFound,
                "Tray icon missing; configure bundle icon to enable system tray",
            )
            .into());
        }
    };

    {
        let toggle_item = toggle_item.clone();
        let autostart_item = autostart_item.clone();
        spawn(async move {
            if let Ok(status) = get_context_menu_status().await {
                let text = if status.enabled {
                    "Disable Explorer Context Menu"
                } else {
                    "Enable Explorer Context Menu"
                };
                let _ = toggle_item.set_text(text);
            }

            if let Ok(status) = get_autostart_status().await {
                let text = if status.enabled {
                    "Disable Autostart with Windows"
                } else {
                    "Enable Autostart with Windows"
                };
                let _ = autostart_item.set_text(text);
            }
        });
    }

    TrayIconBuilder::new()
        .icon(icon)
        .menu(&menu)
        .show_menu_on_left_click(false)
        .on_menu_event({
            let toggle_item = toggle_item.clone();
            let autostart_item = autostart_item.clone();
            move |app, event| {
            match event.id.as_ref() {
                "quit" => app.exit(0),
                "toggle_context_menu" => {
                    let app_handle = app.clone();
                    let toggle_item = toggle_item.clone();
                    spawn(async move {
                        let status = get_context_menu_status().await;
                        let currently_enabled = status
                            .as_ref()
                            .map(|s| s.enabled)
                            .unwrap_or(false);

                        if currently_enabled {
                            let result = unregister_context_menu().await;
                            match result {
                                Ok(res) => {
                                    let _ = toggle_item.set_text("Enable Explorer Context Menu");
                                    log_event(
                                        "tray_context_menu_unregister",
                                        json!({"status": res.success, "message": res.message}),
                                    );
                                    let _ = app_handle.emit_to(
                                        "main",
                                        "tray_context_menu_update",
                                        json!({"success": res.success, "message": res.message}),
                                    );
                                }
                                Err(e) => {
                                    log_event(
                                        "tray_context_menu_unregister",
                                        json!({"status": false, "message": e}),
                                    );
                                    let _ = app_handle.emit_to(
                                        "main",
                                        "tray_context_menu_update",
                                        json!({"success": false, "message": e}),
                                    );
                                }
                            }
                        } else {
                            let result = register_context_menu().await;
                            match result {
                                Ok(res) => {
                                    let _ = toggle_item.set_text("Disable Explorer Context Menu");
                                    log_event(
                                        "tray_context_menu_register",
                                        json!({"status": res.success, "message": res.message}),
                                    );
                                    let _ = app_handle.emit_to(
                                        "main",
                                        "tray_context_menu_update",
                                        json!({"success": res.success, "message": res.message}),
                                    );
                                }
                                Err(e) => {
                                    log_event(
                                        "tray_context_menu_register",
                                        json!({"status": false, "message": e}),
                                    );
                                    let _ = app_handle.emit_to(
                                        "main",
                                        "tray_context_menu_update",
                                        json!({"success": false, "message": e}),
                                    );
                                }
                            }
                        }
                    });
                }
                "toggle_autostart" => {
                    let app_handle = app.clone();
                    let autostart_item = autostart_item.clone();
                    spawn(async move {
                        let status = get_autostart_status().await;
                        let currently_enabled = status
                            .as_ref()
                            .map(|s| s.enabled)
                            .unwrap_or(false);

                        if currently_enabled {
                            let result = unregister_autostart().await;
                            match result {
                                Ok(res) => {
                                    let _ = autostart_item.set_text("Enable Autostart with Windows");
                                    log_event(
                                        "autostart_unregister",
                                        json!({"status": res.success, "message": res.message}),
                                    );
                                    let _ = app_handle.emit_to(
                                        "main",
                                        "tray_autostart_update",
                                        json!({"success": res.success, "message": res.message}),
                                    );
                                }
                                Err(e) => {
                                    log_event(
                                        "autostart_unregister",
                                        json!({"status": false, "message": e}),
                                    );
                                    let _ = app_handle.emit_to(
                                        "main",
                                        "tray_autostart_update",
                                        json!({"success": false, "message": e}),
                                    );
                                }
                            }
                        } else {
                            let result = register_autostart().await;
                            match result {
                                Ok(res) => {
                                    let _ = autostart_item.set_text("Disable Autostart with Windows");
                                    log_event(
                                        "autostart_register",
                                        json!({"status": res.success, "message": res.message}),
                                    );
                                    let _ = app_handle.emit_to(
                                        "main",
                                        "tray_autostart_update",
                                        json!({"success": res.success, "message": res.message}),
                                    );
                                }
                                Err(e) => {
                                    log_event(
                                        "autostart_register",
                                        json!({"status": false, "message": e}),
                                    );
                                    let _ = app_handle.emit_to(
                                        "main",
                                        "tray_autostart_update",
                                        json!({"success": false, "message": e}),
                                    );
                                }
                            }
                        }
                    });
                }
                _ => {}
            }
        }
        })
        .on_tray_icon_event(|tray, event| {
            if let TrayIconEvent::Click {
                button: MouseButton::Left,
                button_state: MouseButtonState::Up,
                ..
            } = event
            {
                let app = tray.app_handle();
                if let Some(window) = app.get_webview_window("main") {
                    match window.is_visible() {
                        Ok(true) => {
                            let _ = window.hide();
                        }
                        Ok(false) => {
                            let _ = window.show();
                        }
                        Err(_) => {}
                    }
                }
            }
        })
        .build(app)?;

    Ok(())
}
