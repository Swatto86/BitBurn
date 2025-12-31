use serde_json::json;
use std::collections::HashSet;
use std::fs;
use std::path::{Path, PathBuf};
use tauri::{AppHandle, Emitter, Manager};
use thiserror::Error;

/// Payload delivered to the frontend when a context-menu wipe is invoked.
#[derive(Debug, Clone, serde::Serialize)]
pub struct ContextWipePayload {
    pub paths: Vec<String>,
    pub invalid: Vec<String>,
    pub source: String,
}

#[derive(Debug, Error)]
pub enum ContextMenuError {
    #[cfg(not(windows))]
    #[error("context menu not supported on this platform")]
    UnsupportedPlatform,
    #[error("missing executable path")]
    MissingExecutablePath,
    #[cfg(windows)]
    #[error("registry error: {0}")]
    Registry(String),
}

#[cfg(windows)]
use winreg::{enums::{HKEY_CURRENT_USER, KEY_READ}, RegKey};

#[cfg(windows)]
fn context_menu_keys() -> (String, String) {
    let base = std::env::var("BITBURN_CONTEXT_ROOT")
        .unwrap_or_else(|_| "Software\\Classes".to_string());
    let file_key = format!("{}\\*\\shell\\BitBurn", base);
    let folder_key = format!("{}\\Directory\\shell\\BitBurn", base);
    (file_key, folder_key)
}

#[cfg(windows)]
fn write_context_menu_for_target(root_key: &str, exe_path: &Path) -> Result<(), ContextMenuError> {
    let hkcu = RegKey::predef(HKEY_CURRENT_USER);

    // Clear existing tree first to ensure a clean state
    let _ = hkcu.delete_subkey_all(root_key);

    let (bitburn_key, _) = hkcu
        .create_subkey(root_key)
        .map_err(|e| ContextMenuError::Registry(e.to_string()))?;
    bitburn_key
        .set_value("MUIVerb", &"BitBurn")
        .map_err(|e| ContextMenuError::Registry(e.to_string()))?;
    bitburn_key
        .set_value("Icon", &exe_path.display().to_string())
        .map_err(|e| ContextMenuError::Registry(e.to_string()))?;

    let shred_key_path = format!("{}\\shell\\Shred", root_key);
    let (shred_key, _) = hkcu
        .create_subkey(&shred_key_path)
        .map_err(|e| ContextMenuError::Registry(e.to_string()))?;
    shred_key
        .set_value("MUIVerb", &"Shred")
        .map_err(|e| ContextMenuError::Registry(e.to_string()))?;
    shred_key
        .set_value("Icon", &exe_path.display().to_string())
        .map_err(|e| ContextMenuError::Registry(e.to_string()))?;

    let algorithm_key_path = format!("{}\\shell\\Shred\\shell\\ChooseShredAlgorithm", root_key);
    let (algorithm_key, _) = hkcu
        .create_subkey(&algorithm_key_path)
        .map_err(|e| ContextMenuError::Registry(e.to_string()))?;
    algorithm_key
        .set_value("MUIVerb", &"Choose Shred Algorithm")
        .map_err(|e| ContextMenuError::Registry(e.to_string()))?;
    algorithm_key
        .set_value("Icon", &exe_path.display().to_string())
        .map_err(|e| ContextMenuError::Registry(e.to_string()))?;

    let command_path = format!("{}\\command", algorithm_key_path);
    let (command_key, _) = hkcu
        .create_subkey(&command_path)
        .map_err(|e| ContextMenuError::Registry(e.to_string()))?;
    let command_value = format!("\"{}\" --context-wipe \"%V\"", exe_path.display());
    command_key
        .set_value("", &command_value)
        .map_err(|e| ContextMenuError::Registry(e.to_string()))?;

    Ok(())
}

#[cfg(windows)]
pub fn enable_context_menu(exe_path: &Path) -> Result<(), ContextMenuError> {
    let (file_key, folder_key) = context_menu_keys();
    write_context_menu_for_target(&file_key, exe_path)?;
    write_context_menu_for_target(&folder_key, exe_path)?;
    Ok(())
}

#[cfg(windows)]
pub fn disable_context_menu() -> Result<(), ContextMenuError> {
    let hkcu = RegKey::predef(HKEY_CURRENT_USER);
    let (file_key, folder_key) = context_menu_keys();
    let _ = hkcu.delete_subkey_all(file_key);
    let _ = hkcu.delete_subkey_all(folder_key);
    Ok(())
}

#[cfg(windows)]
pub fn is_context_menu_enabled() -> Result<bool, ContextMenuError> {
    let hkcu = RegKey::predef(HKEY_CURRENT_USER);
    let (file_key, folder_key) = context_menu_keys();
    Ok(hkcu.open_subkey_with_flags(file_key, KEY_READ).is_ok()
        && hkcu
            .open_subkey_with_flags(folder_key, KEY_READ)
            .is_ok())
}

#[cfg(not(windows))]
pub fn enable_context_menu(_: &Path) -> Result<(), ContextMenuError> {
    Err(ContextMenuError::UnsupportedPlatform)
}

#[cfg(not(windows))]
pub fn disable_context_menu() -> Result<(), ContextMenuError> {
    Err(ContextMenuError::UnsupportedPlatform)
}

#[cfg(not(windows))]
pub fn is_context_menu_enabled() -> Result<bool, ContextMenuError> {
    Err(ContextMenuError::UnsupportedPlatform)
}

pub fn resolve_executable_path() -> Result<PathBuf, ContextMenuError> {
    std::env::current_exe()
        .map_err(|_| ContextMenuError::MissingExecutablePath)
        .map(|p| p.to_path_buf())
}

pub(crate) fn collect_context_paths(args: &[String]) -> Vec<String> {
    let mut results = Vec::new();
    if let Some(index) = args.iter().position(|arg| arg == "--context-wipe") {
        for entry in args.iter().skip(index + 1) {
            if entry.starts_with("--") {
                continue;
            }

            // Windows `%V` may deliver multiple selections in a single argument
            // separated by newlines, pipes, or semicolons. Split generously.
            for part in entry
                .split(|c| c == '|' || c == '\n' || c == '\r' || c == ';')
                .filter(|s| !s.is_empty())
            {
                results.push(part.to_string());
            }
        }
    }
    results
}

pub(crate) fn sanitize_context_paths(raw_paths: Vec<String>) -> ContextWipePayload {
    let mut seen = HashSet::new();
    let mut valid = Vec::new();
    let mut invalid = Vec::new();

    for raw in raw_paths {
        let trimmed = raw.trim();
        if trimmed.is_empty() {
            continue;
        }

        if trimmed.starts_with("\\\\") {
            invalid.push(format!("Network paths are not supported: {}", trimmed));
            continue;
        }

        let candidate = PathBuf::from(trimmed);
        if !candidate.exists() {
            invalid.push(format!("Path not found: {}", trimmed));
            continue;
        }

        match fs::symlink_metadata(&candidate) {
            Ok(metadata) => {
                if metadata.file_type().is_symlink() {
                    invalid.push(format!("Symbolic links are not supported: {}", trimmed));
                    continue;
                }
            }
            Err(err) => {
                invalid.push(format!("Failed to inspect {}: {}", trimmed, err));
                continue;
            }
        }

        let canonical = candidate
            .canonicalize()
            .unwrap_or_else(|_| candidate.clone());

        let canonical_str = match canonical.to_str() {
            Some(val) => val.to_string(),
            None => {
                invalid.push(format!("Unsupported path encoding: {}", trimmed));
                continue;
            }
        };

        if seen.insert(canonical_str.clone()) {
            valid.push(canonical_str);
        }
    }

    ContextWipePayload {
        paths: valid,
        invalid,
        source: "context-menu".to_string(),
    }
}

pub fn dispatch_context_wipe(app: &AppHandle, payload: ContextWipePayload) {
    if payload.paths.is_empty() && payload.invalid.is_empty() {
        return;
    }

    let handle = app.clone();
    tauri::async_runtime::spawn(async move {
        let _ = handle.emit("context_wipe_request", payload.clone());
        if let Some(window) = handle.get_webview_window("main") {
            let _ = window.show();
            let _ = window.set_focus();
        }
    });
}

pub fn handle_context_invocation(app: &AppHandle, argv: &[String]) {
    let raw_paths = collect_context_paths(argv);
    if raw_paths.is_empty() {
        return;
    }

    let payload = sanitize_context_paths(raw_paths);
    dispatch_context_wipe(app, payload);
}

pub fn process_cli_side_effects<F>(argv: &[String], mut log_event: F) -> Option<i32>
where
    F: FnMut(&str, serde_json::Value),
{
    #[cfg(windows)]
    {
        if argv.iter().any(|a| a == "--register-context-menu") {
            let status = resolve_executable_path()
                .and_then(|exe| enable_context_menu(&exe))
                .map(|_| {
                    log_event("context_menu_register_cli", json!({"status": "success"}));
                    println!("Context menu registered");
                    0
                })
                .unwrap_or_else(|e| {
                    log_event("context_menu_register_cli", json!({"status": "error", "message": e.to_string()}));
                    eprintln!("Failed to register context menu: {}", e);
                    1
                });
            return Some(status);
        }

        if argv.iter().any(|a| a == "--unregister-context-menu") {
            let status = disable_context_menu()
                .map(|_| {
                    log_event("context_menu_unregister_cli", json!({"status": "success"}));
                    println!("Context menu removed");
                    0
                })
                .unwrap_or_else(|e| {
                    log_event("context_menu_unregister_cli", json!({"status": "error", "message": e.to_string()}));
                    eprintln!("Failed to remove context menu: {}", e);
                    1
                });
            return Some(status);
        }
    }

    #[cfg(not(windows))]
    {
        let _ = log_event;
        let _ = argv;
    }

    None
}

#[cfg(test)]
mod tests {

    #[cfg(not(windows))]
    #[test]
    fn register_context_menu_is_unavailable_on_non_windows() {
        let result = tauri::async_runtime::block_on(super::register_context_menu()).expect("command should return result");
        assert!(!result.success);
        assert!(result.message.contains("not available"));
    }

    #[cfg(not(windows))]
    #[test]
    fn unregister_context_menu_is_unavailable_on_non_windows() {
        let result = tauri::async_runtime::block_on(super::unregister_context_menu()).expect("command should return result");
        assert!(!result.success);
        assert!(result.message.contains("not available"));
    }

    #[cfg(not(windows))]
    #[test]
    fn get_context_menu_status_is_unavailable_on_non_windows() {
        let status = tauri::async_runtime::block_on(super::get_context_menu_status()).expect("command should return result");
        assert!(!status.enabled);
        assert!(status.message.contains("not available"));
    }
}

/// Register the Explorer context menu entries (Windows only).
#[tauri::command]
pub async fn register_context_menu() -> Result<crate::WipeResult, String> {
    #[cfg(windows)]
    {
        let exe_path = resolve_executable_path().map_err(|e| e.to_string())?;
        enable_context_menu(&exe_path).map_err(|e| e.to_string())?;
        crate::log_event("context_menu_register", json!({"status": "success"}));

        return Ok(crate::WipeResult {
            success: true,
            message: "Context menu registered for files and folders".to_string(),
        });
    }

    #[cfg(not(windows))]
    {
        Ok(crate::WipeResult {
            success: false,
            message: "Context menu not available on this platform".to_string(),
        })
    }
}

/// Unregister the Explorer context menu entries (Windows only).
#[tauri::command]
pub async fn unregister_context_menu() -> Result<crate::WipeResult, String> {
    #[cfg(windows)]
    {
        disable_context_menu().map_err(|e| e.to_string())?;
        crate::log_event("context_menu_unregister", json!({"status": "success"}));

        return Ok(crate::WipeResult {
            success: true,
            message: "Context menu removed".to_string(),
        });
    }

    #[cfg(not(windows))]
    {
        Ok(crate::WipeResult {
            success: false,
            message: "Context menu not available on this platform".to_string(),
        })
    }
}

/// Report whether the Explorer context menu entries are currently installed.
#[tauri::command]
pub async fn get_context_menu_status() -> Result<crate::ContextMenuStatus, String> {
    #[cfg(windows)]
    {
        let enabled = is_context_menu_enabled().map_err(|e| e.to_string())?;
        let message = if enabled {
            "Context menu is registered".to_string()
        } else {
            "Context menu is not registered".to_string()
        };

        return Ok(crate::ContextMenuStatus { enabled, message });
    }

    #[cfg(not(windows))]
    {
        Ok(crate::ContextMenuStatus {
            enabled: false,
            message: "Context menu not available on this platform".to_string(),
        })
    }
}
