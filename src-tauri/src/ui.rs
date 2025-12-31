use serde_json::json;
use std::io;
use tauri::{
    menu::{Menu, MenuItem},
    tray::{MouseButton, MouseButtonState, TrayIconBuilder, TrayIconEvent},
    AppHandle, Manager, WindowEvent,
};

use crate::log_event;

/// Initialize window behavior and system tray for the application.
/// - Centers and resizes the main window to 80% height of the current monitor.
/// - Hooks close requests to hide the window instead of quitting.
/// - Builds a tray icon with a Quit menu and click-to-toggle visibility.
pub fn init_ui(app: &AppHandle) -> tauri::Result<()> {
    setup_window(app)?;
    build_tray(app)?;
    Ok(())
}

fn setup_window(app: &AppHandle) -> tauri::Result<()> {
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
            let _ = window_clone.show();
            let _ = window_clone.set_focus();
        });
    }

    Ok(())
}

fn build_tray(app: &AppHandle) -> tauri::Result<()> {
    let quit_item = MenuItem::with_id(app, "quit", "Quit", true, None::<&str>)?;
    let menu = Menu::with_items(app, &[&quit_item])?;

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

    TrayIconBuilder::new()
        .icon(icon)
        .menu(&menu)
        .show_menu_on_left_click(false)
        .on_menu_event(|app, event| {
            if event.id.as_ref() == "quit" {
                app.exit(0);
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
