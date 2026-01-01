use serde_json::json;
use std::path::{Path, PathBuf};
use thiserror::Error;

#[derive(Debug, Error)]
pub enum AutostartError {
    #[cfg(not(windows))]
    #[error("autostart not supported on this platform")]
    UnsupportedPlatform,
    #[error("missing executable path")]
    MissingExecutablePath,
    #[cfg(windows)]
    #[error("registry error: {0}")]
    Registry(String),
}

#[cfg(windows)]
use winreg::{
    enums::{HKEY_CURRENT_USER, KEY_READ, KEY_WRITE},
    RegKey,
};

#[cfg(windows)]
const RUN_KEY: &str = "Software\\Microsoft\\Windows\\CurrentVersion\\Run";

#[cfg(windows)]
fn write_autostart(exe_path: &Path) -> Result<(), AutostartError> {
    let hkcu = RegKey::predef(HKEY_CURRENT_USER);
    let (key, _) = hkcu
        .create_subkey_with_flags(RUN_KEY, KEY_WRITE)
        .map_err(|e| AutostartError::Registry(e.to_string()))?;

    let value = exe_path.display().to_string();
    key.set_value("BitBurn", &value)
        .map_err(|e| AutostartError::Registry(e.to_string()))?;
    Ok(())
}

#[cfg(windows)]
fn remove_autostart() -> Result<(), AutostartError> {
    let hkcu = RegKey::predef(HKEY_CURRENT_USER);
    if let Ok(key) = hkcu.open_subkey_with_flags(RUN_KEY, KEY_WRITE) {
        let _ = key.delete_value("BitBurn");
    }
    Ok(())
}

#[cfg(windows)]
fn is_autostart_enabled() -> Result<bool, AutostartError> {
    let hkcu = RegKey::predef(HKEY_CURRENT_USER);
    if let Ok(key) = hkcu.open_subkey_with_flags(RUN_KEY, KEY_READ) {
        if let Ok::<String, _>(_val) = key.get_value("BitBurn") {
            return Ok(true);
        }
    }
    Ok(false)
}

#[cfg(not(windows))]
fn write_autostart(_: &Path) -> Result<(), AutostartError> {
    Err(AutostartError::UnsupportedPlatform)
}

#[cfg(not(windows))]
fn remove_autostart() -> Result<(), AutostartError> {
    Err(AutostartError::UnsupportedPlatform)
}

#[cfg(not(windows))]
fn is_autostart_enabled() -> Result<bool, AutostartError> {
    Err(AutostartError::UnsupportedPlatform)
}

fn resolve_executable_path() -> Result<PathBuf, AutostartError> {
    std::env::current_exe()
        .map_err(|_| AutostartError::MissingExecutablePath)
        .map(|p| p.to_path_buf())
}

/// Enable BitBurn autostart on Windows by writing a Run key entry.
#[tauri::command]
pub async fn register_autostart() -> Result<crate::WipeResult, String> {
    #[cfg(windows)]
    {
        let exe_path = resolve_executable_path().map_err(|e| e.to_string())?;
        write_autostart(&exe_path).map_err(|e| e.to_string())?;
        crate::log_event("autostart_register", json!({"status": "success"}));

        return Ok(crate::WipeResult {
            success: true,
            message: "Autostart enabled".to_string(),
        });
    }

    #[cfg(not(windows))]
    {
        Ok(crate::WipeResult {
            success: false,
            message: "Autostart not supported on this platform".to_string(),
        })
    }
}

/// Disable BitBurn autostart by removing the Run key entry.
#[tauri::command]
pub async fn unregister_autostart() -> Result<crate::WipeResult, String> {
    #[cfg(windows)]
    {
        remove_autostart().map_err(|e| e.to_string())?;
        crate::log_event("autostart_unregister", json!({"status": "success"}));

        return Ok(crate::WipeResult {
            success: true,
            message: "Autostart disabled".to_string(),
        });
    }

    #[cfg(not(windows))]
    {
        Ok(crate::WipeResult {
            success: false,
            message: "Autostart not supported on this platform".to_string(),
        })
    }
}

/// Report whether autostart is currently enabled.
#[tauri::command]
pub async fn get_autostart_status() -> Result<crate::AutostartStatus, String> {
    #[cfg(windows)]
    {
        let enabled = is_autostart_enabled().map_err(|e| e.to_string())?;
        let message = if enabled {
            "Autostart is enabled".to_string()
        } else {
            "Autostart is disabled".to_string()
        };

        return Ok(crate::AutostartStatus { enabled, message });
    }

    #[cfg(not(windows))]
    {
        Ok(crate::AutostartStatus {
            enabled: false,
            message: "Autostart not supported on this platform".to_string(),
        })
    }
}

#[cfg(test)]
mod tests {
    #[cfg(not(windows))]
    #[test]
    fn autostart_is_unavailable_on_non_windows() {
        let status = tauri::async_runtime::block_on(super::get_autostart_status()).expect("command should return result");
        assert!(!status.enabled);
        assert!(status.message.contains("not supported") || status.message.contains("disabled"));
    }
}
