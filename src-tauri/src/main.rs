// Prevents additional console window on Windows in release
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

use rand::RngCore;
use serde::{Deserialize, Serialize};
use serde_json::json;
use std::fs::{self, OpenOptions};
use std::io::{self, Seek, SeekFrom, Write};
use std::path::{Path, PathBuf};
use tauri::{Emitter, Listener, Manager, Runtime};
use tauri::async_runtime::spawn_blocking;
use walkdir::WalkDir;
use std::fmt;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use sysinfo::{DiskExt, System, SystemExt};
mod platform;
mod ui;

use platform::context_menu::{
    get_context_menu_status,
    handle_context_invocation,
    process_cli_side_effects,
    register_context_menu,
    unregister_context_menu,
};
use platform::autostart::{get_autostart_status, register_autostart, unregister_autostart, AUTOSTART_FLAG};

/// Errors that can occur while securely wiping files.
#[derive(Debug)]
pub enum WipeError {
    PathNotFound,
    Io(std::io::Error),
    InvalidPasses,
}

impl fmt::Display for WipeError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            WipeError::PathNotFound => write!(f, "Path not found"),
            WipeError::Io(err) => write!(f, "IO error: {}", err),
            WipeError::InvalidPasses => write!(f, "Invalid number of passes"),
        }
    }
}

impl std::error::Error for WipeError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            WipeError::Io(err) => Some(err),
            _ => None,
        }
    }
}

/// Supported wipe algorithms exposed to the frontend.
/// Each variant maps to a specific pass count and pattern strategy enforced in the backend.
#[derive(Debug, Serialize, Deserialize, Clone)]
pub enum WipeAlgorithm {
    NistClear,      // NIST 800-88 Clear: 1 pass zeros (replaces Basic)
    NistPurge,      // NIST 800-88 Purge: 3 pass overwrite (replaces DOD)
    Gutmann,        // 35 pass: Gutmann pattern (kept for legacy/specific needs)
    Random,         // N passes of random data (replaces DOD_E and custom needs)
}

/// Progress payload emitted to the UI during wipe operations.
#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct WipeProgress {
    current_pass: u32,
    total_passes: u32,
    bytes_processed: u64,
    total_bytes: u64,
    current_algorithm: String,
    current_pattern: String,
    percentage: f32,
    estimated_total_bytes: Option<u64>,
}

impl WipeProgress {
    fn new(total_passes: u32, total_bytes: u64, current_algorithm: &str) -> Self {
        WipeProgress {
            current_pass: 1,
            total_passes,
            bytes_processed: 0,
            total_bytes,
            current_algorithm: current_algorithm.to_string(),
            current_pattern: String::new(),
            percentage: 0.0,
            estimated_total_bytes: None,
        }
    }

    fn update(&mut self, bytes_processed: u64, pattern: &str) {
        self.bytes_processed = bytes_processed;
        self.current_pattern = pattern.to_string();
        if let Some(est_total) = self.estimated_total_bytes {
            self.percentage = (bytes_processed as f32 / est_total as f32) * 100.0;
        } else {
            self.percentage = (bytes_processed as f32 / self.total_bytes as f32) * 100.0;
        }
    }
}

/// User-facing result payload returned by wipe commands.
/// Carries a success flag and human-readable status message for UI display.
#[derive(Serialize)]
pub struct WipeResult {
    success: bool,
    message: String,
}

fn cancelled_wipe_result() -> WipeResult {
    WipeResult {
        success: false,
        message: "Operation cancelled by user".to_string(),
    }
}

fn free_space_error_result(message: impl Into<String>) -> WipeResult {
    WipeResult {
        success: false,
        message: message.into(),
    }
}

/// Context menu registration status returned to the frontend.
/// Reports whether shell integration is enabled and any explanatory message.
#[derive(Serialize)]
pub struct ContextMenuStatus {
    enabled: bool,
    message: String,
}

/// Autostart registration status returned to the frontend.
#[derive(Serialize)]
pub struct AutostartStatus {
    enabled: bool,
    message: String,
}

pub(crate) fn log_event(event: &str, fields: serde_json::Value) {
    if let Ok(serialized) = serde_json::to_string(&json!({ "event": event, "fields": fields })) {
        println!("{}", serialized);
    }
}

/// Platform info reported to the frontend for capability gating.
#[derive(Debug, Serialize, Clone)]
pub struct PlatformInfo {
    is_windows: bool,
    os: String,
}


fn secure_wipe_file<F>(path: &Path, passes: u32, algorithm: &WipeAlgorithm, mut progress_callback: F) -> Result<(), WipeError>
where
    F: FnMut(WipeProgress),
{
    let cancelled = Arc::new(AtomicBool::new(false));

    let check_cancelled = || {
        if cancelled.load(Ordering::SeqCst) {
            return Err(WipeError::Io(std::io::Error::new(
                std::io::ErrorKind::Interrupted,
                "Operation cancelled by user"
            )));
        }
        Ok(())
    };

    if path.is_symlink() {
        return Err(WipeError::Io(std::io::Error::new(
            std::io::ErrorKind::InvalidInput,
            "Cannot wipe symbolic links"
        )));
    }

    if !path.exists() {
        return Err(WipeError::PathNotFound);
    }

    if passes == 0 {
        return Err(WipeError::InvalidPasses);
    }

    // Try to open file with minimal permissions first to check access
    match OpenOptions::new().write(true).open(path) {
        Ok(_) => {},
        Err(e) => {
            if e.kind() == std::io::ErrorKind::PermissionDenied {
                return Err(WipeError::Io(std::io::Error::new(
                    std::io::ErrorKind::PermissionDenied,
                    "Access denied. The file might be in use or require administrator privileges."
                )));
            }
            return Err(WipeError::Io(e));
        }
    }

    let mut file = OpenOptions::new()
        .write(true)
        .read(true)
        .open(path)
        .map_err(|e| {
            if e.kind() == std::io::ErrorKind::PermissionDenied {
                WipeError::Io(std::io::Error::new(
                    std::io::ErrorKind::PermissionDenied,
                    "Access denied. The file might be in use or require administrator privileges."
                ))
            } else {
                WipeError::Io(e)
            }
        })?;

    let file_size = file.metadata().map_err(WipeError::Io)?.len();
    let mut rng = rand::thread_rng();
    let mut progress = WipeProgress::new(
        passes,
        file_size,
        match algorithm {
            WipeAlgorithm::NistClear => "NIST 800-88 Clear",
            WipeAlgorithm::NistPurge => "NIST 800-88 Purge",
            WipeAlgorithm::Gutmann => "Gutmann",
            WipeAlgorithm::Random => "Random",
        }
    );

    // Increase buffer size to 1MB for better performance and smooth updates
    const BUFFER_SIZE: u64 = 1024 * 1024; // 1MB
    let mut last_progress_update = std::time::Instant::now();
    let progress_update_interval = std::time::Duration::from_millis(16); // ~60 fps

    match algorithm {
        WipeAlgorithm::NistClear => {
            // NIST 800-88 Clear: Single pass with zeros
            progress.update(0, "NIST 800-88 Clear - Writing zeros");
            progress_callback(progress.clone());

            let buffer = vec![0u8; BUFFER_SIZE as usize];
            for chunk_start in (0..file_size).step_by(BUFFER_SIZE as usize) {
                check_cancelled()?;
                let chunk_size = std::cmp::min(BUFFER_SIZE, file_size - chunk_start);
                file.write_all(&buffer[..chunk_size as usize]).map_err(WipeError::Io)?;

                // Update progress at most every 16ms for smooth animation
                if last_progress_update.elapsed() >= progress_update_interval {
                    progress.update(
                        chunk_start + chunk_size,
                        &format!("NIST 800-88 Clear - Writing zeros ({:.2} MB / {:.2} MB)",
                            (chunk_start + chunk_size) as f64 / 1024.0 / 1024.0,
                            file_size as f64 / 1024.0 / 1024.0
                        )
                    );
                    progress_callback(progress.clone());
                    last_progress_update = std::time::Instant::now();
                }
            }
            file.sync_all().map_err(WipeError::Io)?;
            
            // Final cleanup
            check_cancelled()?;
            progress.update(file_size, "Finalizing NIST 800-88 Clear wipe");
            progress_callback(progress);
        },
        WipeAlgorithm::NistPurge => {
            // NIST 800-88 Purge: Three-pass overwrite
            let patterns = [
                (0x00, false, "zeros"),
                (0xFF, false, "ones"),
                (0x00, true, "random data")
            ];

            for (pass, &(pattern, is_random, pattern_type)) in patterns.iter().enumerate() {
                check_cancelled()?;
                progress.current_pass = (pass + 1) as u32;
                let desc = format!("NIST 800-88 Purge - Writing {} (Pass {}/3)", pattern_type, pass + 1);
                progress.update(0, &desc);
                progress_callback(progress.clone());

                file.seek(SeekFrom::Start(0)).map_err(WipeError::Io)?;
                let mut buffer = vec![pattern; BUFFER_SIZE as usize];

                for chunk_start in (0..file_size).step_by(BUFFER_SIZE as usize) {
                    check_cancelled()?;
                    let chunk_size = std::cmp::min(BUFFER_SIZE, file_size - chunk_start);
                    if is_random {
                        rng.fill_bytes(&mut buffer[..chunk_size as usize]);
                    }
                    file.write_all(&buffer[..chunk_size as usize]).map_err(WipeError::Io)?;

                    // Update progress at most every 16ms for smooth animation
                    if last_progress_update.elapsed() >= progress_update_interval {
                        progress.update(
                            chunk_start + chunk_size,
                            &format!("NIST 800-88 Purge - Writing {} (Pass {}/3) - {:.2} MB / {:.2} MB",
                                pattern_type,
                                pass + 1,
                                (chunk_start + chunk_size) as f64 / 1024.0 / 1024.0,
                                file_size as f64 / 1024.0 / 1024.0
                            )
                        );
                        progress_callback(progress.clone());
                        last_progress_update = std::time::Instant::now();
                    }
                }
                file.sync_all().map_err(WipeError::Io)?;
            }
            
            // Final cleanup
            check_cancelled()?;
            progress.update(file_size, "Finalizing NIST 800-88 Purge wipe");
            progress_callback(progress);
        },
        WipeAlgorithm::Gutmann => {
            // Gutmann 35-pass pattern
            // Reference: https://en.wikipedia.org/wiki/Gutmann_method
            let patterns: &[(Vec<u8>, bool, &str)] = &[
                // Passes 1-4: Random
                (vec![0x00], true, "Random data (Pass 1/35)"),
                (vec![0x00], true, "Random data (Pass 2/35)"),
                (vec![0x00], true, "Random data (Pass 3/35)"),
                (vec![0x00], true, "Random data (Pass 4/35)"),
                
                // Passes 5-31: Fixed patterns
                (vec![0x55, 0xAA, 0x55, 0xAA], false, "Pattern 5/35: 0x55 0xAA"),
                (vec![0xAA, 0x55, 0xAA, 0x55], false, "Pattern 6/35: 0xAA 0x55"),
                (vec![0x92, 0x49, 0x24], false, "Pattern 7/35: 0x92 0x49 0x24"),
                (vec![0x49, 0x24, 0x92], false, "Pattern 8/35: 0x49 0x24 0x92"),
                (vec![0x24, 0x92, 0x49], false, "Pattern 9/35: 0x24 0x92 0x49"),
                (vec![0x00], false, "Pattern 10/35: 0x00"),
                (vec![0x11], false, "Pattern 11/35: 0x11"),
                (vec![0x22], false, "Pattern 12/35: 0x22"),
                (vec![0x33], false, "Pattern 13/35: 0x33"),
                (vec![0x44], false, "Pattern 14/35: 0x44"),
                (vec![0x55], false, "Pattern 15/35: 0x55"),
                (vec![0x66], false, "Pattern 16/35: 0x66"),
                (vec![0x77], false, "Pattern 17/35: 0x77"),
                (vec![0x88], false, "Pattern 18/35: 0x88"),
                (vec![0x99], false, "Pattern 19/35: 0x99"),
                (vec![0xAA], false, "Pattern 20/35: 0xAA"),
                (vec![0xBB], false, "Pattern 21/35: 0xBB"),
                (vec![0xCC], false, "Pattern 22/35: 0xCC"),
                (vec![0xDD], false, "Pattern 23/35: 0xDD"),
                (vec![0xEE], false, "Pattern 24/35: 0xEE"),
                (vec![0xFF], false, "Pattern 25/35: 0xFF"),
                (vec![0x92, 0x49, 0x24], false, "Pattern 26/35: 0x92 0x49 0x24"),
                (vec![0x49, 0x24, 0x92], false, "Pattern 27/35: 0x49 0x24 0x92"),
                (vec![0x24, 0x92, 0x49], false, "Pattern 28/35: 0x24 0x92 0x49"),
                (vec![0x6D, 0xB6, 0xDB], false, "Pattern 29/35: 0x6D 0xB6 0xDB"),
                (vec![0xB6, 0xDB, 0x6D], false, "Pattern 30/35: 0xB6 0xDB 0x6D"),
                (vec![0xDB, 0x6D, 0xB6], false, "Pattern 31/35: 0xDB 0x6D 0xB6"),
                
                // Passes 32-35: Random
                (vec![0x00], true, "Random data (Pass 32/35)"),
                (vec![0x00], true, "Random data (Pass 33/35)"),
                (vec![0x00], true, "Random data (Pass 34/35)"),
                (vec![0x00], true, "Random data (Pass 35/35)")
            ];

            for (pass, &(ref pattern, is_random, desc)) in patterns.iter().enumerate() {
                check_cancelled()?;
                progress.current_pass = (pass + 1) as u32;
                progress.update(0, desc);
                progress_callback(progress.clone());

                file.seek(SeekFrom::Start(0)).map_err(WipeError::Io)?;
                let mut buffer = vec![0u8; BUFFER_SIZE as usize];

                for chunk_start in (0..file_size).step_by(BUFFER_SIZE as usize) {
                    check_cancelled()?;
                    let chunk_size = std::cmp::min(BUFFER_SIZE, file_size - chunk_start) as usize;
                    
                    if is_random {
                        rng.fill_bytes(&mut buffer[..chunk_size]);
                    } else {
                        // Fill the buffer with the repeating pattern
                        for i in 0..chunk_size {
                            buffer[i] = pattern[i % pattern.len()];
                        }
                    }
                    
                    file.write_all(&buffer[..chunk_size]).map_err(WipeError::Io)?;

                    // Update progress at most every 16ms for smooth animation
                    if last_progress_update.elapsed() >= progress_update_interval {
                        progress.update(
                            chunk_start + chunk_size as u64,
                            &format!("{} - {:.2} MB / {:.2} MB",
                                desc,
                                (chunk_start + chunk_size as u64) as f64 / 1024.0 / 1024.0,
                                file_size as f64 / 1024.0 / 1024.0
                            )
                        );
                        progress_callback(progress.clone());
                        last_progress_update = std::time::Instant::now();
                    }
                }
                file.sync_all().map_err(WipeError::Io)?;
            }
            
            // Final cleanup
            check_cancelled()?;
            progress.update(file_size, "Finalizing Gutmann wipe");
            progress_callback(progress);
        },
        WipeAlgorithm::Random => {
            for pass in 1..=passes {
                check_cancelled()?;
                progress.current_pass = pass;
                let desc = format!("Writing random data (Pass {}/{})", pass, passes);
                progress.update(0, &desc);
                progress_callback(progress.clone());

                file.seek(SeekFrom::Start(0)).map_err(WipeError::Io)?;
                let mut buffer = vec![0u8; BUFFER_SIZE as usize];
                for chunk_start in (0..file_size).step_by(BUFFER_SIZE as usize) {
                    check_cancelled()?;
                    let chunk_size = std::cmp::min(BUFFER_SIZE, file_size - chunk_start);
                    rng.fill_bytes(&mut buffer[..chunk_size as usize]);
                    file.write_all(&buffer[..chunk_size as usize]).map_err(WipeError::Io)?;

                    // Update progress at most every 16ms for smooth animation
                    if last_progress_update.elapsed() >= progress_update_interval {
                        progress.update(
                            chunk_start + chunk_size,
                            &format!("Writing random data (Pass {}/{}) - {:.2} MB / {:.2} MB",
                                pass,
                                passes,
                                (chunk_start + chunk_size) as f64 / 1024.0 / 1024.0,
                                file_size as f64 / 1024.0 / 1024.0
                            )
                        );
                        progress_callback(progress.clone());
                        last_progress_update = std::time::Instant::now();
                    }
                }
                file.sync_all().map_err(WipeError::Io)?;
            }
            
            // Final cleanup
            check_cancelled()?;
            progress.update(file_size, "Finalizing random wipe");
            progress_callback(progress);
        },
    }

    // Final cleanup
    check_cancelled()?;
    file.set_len(0).map_err(WipeError::Io)?;
    drop(file);
    fs::remove_file(path).map_err(WipeError::Io)?;

    Ok(())
}

/// Validation errors for drive-root selection when wiping free space.
#[derive(Debug)]
pub enum DriveValidationError {
    PathNotFound,
    NotDriveRoot,
}

impl fmt::Display for DriveValidationError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            DriveValidationError::PathNotFound => write!(f, "Path not found"),
            DriveValidationError::NotDriveRoot => write!(f, "Selected path is not a drive root"),
        }
    }
}

fn validate_drive_path_internal(path: &Path) -> Result<(), DriveValidationError> {
    if !path.exists() {
        return Err(DriveValidationError::PathNotFound);
    }

    // Check if it's a drive root (e.g., "C:\")
    if path.to_string_lossy().matches('\\').count() != 1 {
        return Err(DriveValidationError::NotDriveRoot);
    }

    Ok(())
}

/// Validate that the provided path is an existing drive root (e.g., "C:\").
/// Returns a user-friendly `WipeResult` describing success or the validation failure.
#[tauri::command]
async fn validate_drive_path(path: String) -> Result<WipeResult, String> {
    let path = Path::new(&path);
    
    match validate_drive_path_internal(path) {
        Ok(_) => {
            log_event("validate_drive_path", json!({"status": "success", "path": path.to_string_lossy()}));
            Ok(WipeResult {
                success: true,
                message: "Path validation successful".to_string(),
            })
        }
        Err(e) => {
            log_event("validate_drive_path", json!({"status": "error", "path": path.to_string_lossy(), "message": e.to_string()}));
            Ok(WipeResult {
                success: false,
                message: e.to_string(),
            })
        }
    }
}

/// Show a blocking warning dialog summarizing the wipe request.
/// The dialog warns the user about the impending wipe and returns their confirmation choice.
#[tauri::command]
async fn show_confirmation_dialog<R: Runtime>(
    window: tauri::Window<R>,
    path: String,
    algorithm: String,
    description: String,
) -> Result<bool, String> {
    use tauri_plugin_dialog::DialogExt;

    let message = if path.contains('\n') {
        // File wiping confirmation
        let file_count = path.lines().count();
        format!(
            "You are about to permanently erase {} file(s) using:\n\nAlgorithm: {}\nDescription: {}\n\nTHIS ACTION CANNOT BE UNDONE!\n\nAre you absolutely sure you want to continue?",
            file_count, algorithm, description
        )
    } else {
        // Drive wiping confirmation
        format!(
            "You are about to wipe all free space on the selected drive using:\n\nAlgorithm: {}\nDescription: {}\n\nTHIS ACTION CANNOT BE UNDONE!\n\nAre you absolutely sure you want to continue?",
            algorithm, description
        )
    };

    let confirmed = window
        .dialog()
        .message(&message)
        .kind(tauri_plugin_dialog::MessageDialogKind::Warning)
        .title("⚠️ WARNING ⚠️")
        .buttons(tauri_plugin_dialog::MessageDialogButtons::YesNo)
        .blocking_show();

    Ok(confirmed)
}

/// Report platform information to the frontend for capability gating.
/// Used by the UI to toggle platform-specific controls without leaking OS concerns into core logic.
#[tauri::command]
async fn platform_info() -> Result<PlatformInfo, String> {
    #[cfg(windows)]
    {
        Ok(PlatformInfo {
            is_windows: true,
            os: "windows".to_string(),
        })
    }

    #[cfg(target_os = "macos")]
    {
        Ok(PlatformInfo {
            is_windows: false,
            os: "macos".to_string(),
        })
    }

    #[cfg(target_os = "linux")]
    {
        Ok(PlatformInfo {
            is_windows: false,
            os: "linux".to_string(),
        })
    }

    #[cfg(not(any(windows, target_os = "macos", target_os = "linux")))]
    {
        Ok(PlatformInfo {
            is_windows: false,
            os: "unknown".to_string(),
        })
    }
}

/// Wipe free space by filling a temp file and securely deleting it.
/// Blocks heavy I/O on a worker thread while emitting progress events to the main window.
#[tauri::command]
async fn execute_free_space_wipe<R: Runtime>(
    window: tauri::Window<R>,
    path: String,
    algorithm: WipeAlgorithm,
    passes: u32
) -> Result<WipeResult, String> {
    let window_label = window.label().to_string();
    let app_handle = window.app_handle().clone();
    let cancelled = Arc::new(AtomicBool::new(false));
    let cancel_for_listener = cancelled.clone();

    let _cancel_listener = app_handle.listen("cancel_operation", move |_| {
        cancel_for_listener.store(true, Ordering::SeqCst);
    });

    let path_buf = PathBuf::from(path);
    let algo_for_task = algorithm.clone();

    let join_result = spawn_blocking(move || {
        let path = path_buf;

        log_event(
            "wipe_free_space_start",
            json!({"path": path.to_string_lossy(), "algorithm": format!("{:?}", algo_for_task), "passes": passes}),
        );

        // Validate again just to be safe
        if let Err(e) = validate_drive_path_internal(&path) {
            return Ok(free_space_error_result(e.to_string()));
        }

        let mut sys = System::new_all();
        sys.refresh_disks_list();

        let disk_info = sys
            .disks()
            .iter()
            .find(|disk| path.starts_with(disk.mount_point()))
            .ok_or_else(|| "Could not find disk information".to_string())?;

        let available_space = disk_info.available_space();
        let cancelled_clone = cancelled.clone();
        let app_handle = app_handle.clone();
        let window_label = window_label.clone();
        let progress_callback = move |progress: WipeProgress| {
            if !cancelled_clone.load(Ordering::SeqCst) {
                let _ = app_handle.emit_to(&window_label, "wipe_progress", progress);
            }
        };

        let mut progress = WipeProgress::new(
            passes,
            0,
            match algo_for_task {
                WipeAlgorithm::NistClear => "NIST 800-88 Clear",
                WipeAlgorithm::NistPurge => "NIST 800-88 Purge",
                WipeAlgorithm::Gutmann => "Gutmann",
                WipeAlgorithm::Random => "Random",
            },
        );

        progress.estimated_total_bytes = Some(available_space);

        progress.update(0, "Filling drive space");
        progress_callback(progress.clone());

        let temp_file_path = path.join(".temp_wipe_file");

        if temp_file_path.exists() {
            progress.update(0, "Cleaning up previous temporary file");
            progress_callback(progress.clone());
            if let Err(e) = fs::remove_file(&temp_file_path) {
                return Ok(free_space_error_result(format!(
                    "Failed to remove existing temporary file: {}",
                    e
                )));
            }
        }

        let mut file = match OpenOptions::new().write(true).create(true).open(&temp_file_path) {
            Ok(f) => f,
            Err(e) => {
                return Ok(free_space_error_result(format!(
                    "Failed to create temporary file: {}",
                    e
                )));
            }
        };

        let chunk_size = 1024 * 1024; // 1MB chunks
        let mut buffer = vec![0u8; chunk_size];
        let mut rng = rand::thread_rng();
        let mut total_written = 0u64;
        let mut last_refresh = std::time::Instant::now();
        let mut last_space_used = 0u64;

        loop {
            if cancelled.load(Ordering::SeqCst) {
                let _ = file.sync_all();
                let _ = fs::remove_file(&temp_file_path);
                return Ok(cancelled_wipe_result());
            }

            if last_refresh.elapsed() >= std::time::Duration::from_millis(100) {
                sys.refresh_disks_list();
                if let Some(disk) = sys.disks().iter().find(|disk| path.starts_with(disk.mount_point())) {
                    let current_available = disk.available_space();
                    last_space_used = available_space - current_available;
                }
                last_refresh = std::time::Instant::now();
            }

            rng.fill_bytes(&mut buffer);
            match file.write_all(&buffer) {
                Ok(_) => {
                    total_written += chunk_size as u64;
                    progress.update(last_space_used, &format!("Filling drive space ({} MB written)", total_written / 1024 / 1024));
                    progress_callback(progress.clone());

                    if total_written % (10 * chunk_size as u64) == 0 {
                        if let Err(_) = file.sync_all() {
                            break;
                        }
                    }
                }
                Err(e) => {
                    if e.kind() == io::ErrorKind::StorageFull
                        || e.kind() == io::ErrorKind::OutOfMemory
                        || e.kind() == io::ErrorKind::WriteZero
                    {
                        sys.refresh_disks_list();
                        if let Some(disk) = sys.disks().iter().find(|disk| path.starts_with(disk.mount_point())) {
                            let current_available = disk.available_space();
                            let space_used = available_space - current_available;
                            progress.update(space_used, "Drive space filled");
                            progress_callback(progress.clone());
                        }
                        break;
                    }
                    let _ = fs::remove_file(&temp_file_path);
                    return Ok(free_space_error_result(format!(
                        "Failed to write to temporary file: {}",
                        e
                    )));
                }
            }
        }

        progress.total_bytes = total_written;
        let cancelled_clone = cancelled.clone();
        match secure_wipe_file(&temp_file_path, passes, &algo_for_task, move |p| {
            if !cancelled_clone.load(Ordering::SeqCst) {
                progress_callback(p);
            }
        }) {
            Ok(_) => {
                if cancelled.load(Ordering::SeqCst) {
                    log_event("wipe_free_space_cancelled", json!({"path": path.to_string_lossy()}));
                    Ok(cancelled_wipe_result())
                } else {
                    log_event("wipe_free_space_complete", json!({"path": path.to_string_lossy(), "status": "success"}));
                    Ok(WipeResult {
                        success: true,
                        message: "Successfully wiped free space".to_string(),
                    })
                }
            }
            Err(e) => {
                let _ = fs::remove_file(&temp_file_path);
                log_event(
                    "wipe_free_space_error",
                    json!({"path": path.to_string_lossy(), "message": format!("{}", e)}),
                );
                Ok(free_space_error_result(format!("Failed to wipe free space: {}", e)))
            }
        }
    })
    .await
    .map_err(|e| format!("wipe_free_space task join error: {}", e))?;

    join_result
}

/// Securely wipe files or folders using the selected algorithm.
/// Runs in a blocking task to avoid UI stalls and streams progress to the main window.
#[tauri::command]
async fn wipe_files<R: Runtime>(
    window: tauri::Window<R>,
    paths: Vec<String>,
    passes: u32,
    algorithm: WipeAlgorithm
) -> Result<WipeResult, String> {
    let window_label = window.label().to_string();
    let app_handle = window.app_handle().clone();
    let cancelled = Arc::new(AtomicBool::new(false));
    let cancel_for_listener = cancelled.clone();

    let _cancel_listener = app_handle.listen("cancel_operation", move |_| {
        cancel_for_listener.store(true, Ordering::SeqCst);
    });

    let paths_for_task = paths.clone();
    let algo_for_task = algorithm.clone();

    let join_result = spawn_blocking(move || {
        log_event(
            "wipe_files_start",
            json!({"count": paths_for_task.len(), "algorithm": format!("{:?}", algo_for_task), "passes": passes}),
        );

        let mut total_files = 0;
        let mut failed_files = Vec::new();

        for path_str in paths_for_task {
            if cancelled.load(Ordering::SeqCst) {
                return Ok(cancelled_wipe_result());
            }

            let path = Path::new(&path_str);

            if !path.exists() {
                failed_files.push(format!("Path not found: {}", path_str));
                continue;
            }

            let emit_progress = {
                let app_handle = app_handle.clone();
                let window_label = window_label.clone();
                let cancelled_clone = cancelled.clone();
                move |progress| {
                    if !cancelled_clone.load(Ordering::SeqCst) {
                        let _ = app_handle.emit_to(&window_label, "wipe_progress", progress);
                    }
                }
            };

            if path.is_file() {
                match secure_wipe_file(path, passes, &algo_for_task, emit_progress) {
                    Ok(_) => total_files += 1,
                    Err(e) => failed_files.push(format!("Failed to wipe {}: {}", path_str, e)),
                }
            } else if path.is_dir() {
                let files: Vec<_> = WalkDir::new(path)
                    .into_iter()
                    .filter_map(|e| e.ok())
                    .filter(|e| e.file_type().is_file())
                    .collect();

                for entry in files {
                    if cancelled.load(Ordering::SeqCst) {
                        return Ok(WipeResult {
                            success: false,
                            message: "Operation cancelled by user".to_string(),
                        });
                    }

                    let emit_progress = {
                        let app_handle = app_handle.clone();
                        let window_label = window_label.clone();
                        let cancelled_clone = cancelled.clone();
                        move |progress| {
                            if !cancelled_clone.load(Ordering::SeqCst) {
                                let _ = app_handle.emit_to(&window_label, "wipe_progress", progress);
                            }
                        }
                    };

                    match secure_wipe_file(entry.path(), passes, &algo_for_task, emit_progress) {
                        Ok(_) => total_files += 1,
                        Err(e) => failed_files.push(format!("Failed to wipe {}: {}", entry.path().display(), e)),
                    }
                }

                if let Err(e) = fs::remove_dir_all(path) {
                    failed_files.push(format!("Failed to remove directory {}: {}", path_str, e));
                }
            }
        }

            if cancelled.load(Ordering::SeqCst) {
                let result = cancelled_wipe_result();
                log_event("wipe_files_end", json!({"status": "cancelled", "count": total_files, "errors": failed_files.len()}));
                Ok(result)
            } else if failed_files.is_empty() {
            let result = WipeResult {
                success: true,
                message: format!("Successfully wiped {} files", total_files),
            };
            log_event("wipe_files_end", json!({"status": "success", "count": total_files}));
            Ok(result)
        } else {
            let result = WipeResult {
                success: false,
                message: format!(
                    "Wiped {} files with {} errors:\n{}",
                    total_files,
                    failed_files.len(),
                    failed_files.join("\n")
                ),
            };
            log_event(
                "wipe_files_end",
                json!({"status": "partial", "count": total_files, "errors": failed_files.len()}),
            );
            Ok(result)
        }
    })
    .await
    .map_err(|e| format!("wipe_files task join error: {}", e))?;

    join_result
}

fn main() {
    let args: Vec<String> = std::env::args().collect();
    if let Some(code) = process_cli_side_effects(&args, log_event) {
        std::process::exit(code);
    }

    let launch_hidden = args.iter().any(|arg| arg == AUTOSTART_FLAG);
    let initial_args = args.clone();

    tauri::Builder::default()
        .plugin(tauri_plugin_dialog::init())
        .plugin(tauri_plugin_single_instance::init(|app, argv, _| {
            handle_context_invocation(&app.app_handle(), &argv);
        }))
        .invoke_handler(tauri::generate_handler![
            validate_drive_path,
            show_confirmation_dialog,
            execute_free_space_wipe,
            wipe_files,
            register_context_menu,
            unregister_context_menu,
            get_context_menu_status,
            register_autostart,
            unregister_autostart,
            get_autostart_status,
            platform_info
        ])
        .setup(move |app| {
            handle_context_invocation(&app.app_handle(), &initial_args);
            ui::init_ui(&app.app_handle(), launch_hidden)?;
            Ok(())
        })
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs::{self, File};
    use std::io::{self, Write};
    use std::path::{Path, PathBuf};
    use std::time::{SystemTime, UNIX_EPOCH};
    use std::thread;
    use std::time::Duration;
    use crate::platform::context_menu::{
        collect_context_paths,
        sanitize_context_paths,
        enable_context_menu,
        disable_context_menu,
        is_context_menu_enabled,
    };

    fn get_unique_id() -> u128 {
        thread::sleep(Duration::from_millis(10)); // Ensure unique timestamps
        SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos()
    }

    fn create_test_dir() -> io::Result<PathBuf> {
        let unique_id = get_unique_id();
        let test_dir = std::env::temp_dir().join(format!("BitBurn_test_{}", unique_id));
        fs::create_dir_all(&test_dir)?;
        println!("Created test directory: {:?}", test_dir);
        // Add a small delay to ensure directory is fully created
        thread::sleep(Duration::from_millis(50));
        Ok(test_dir)
    }

    fn create_test_file(dir: &Path, content: &[u8]) -> io::Result<PathBuf> {
        let unique_id = get_unique_id();
        let file_path = dir.join(format!("test_file_{}", unique_id));
        println!("Creating test file at: {:?}", file_path);
        
        // Create parent directory if it doesn't exist
        if let Some(parent) = file_path.parent() {
            fs::create_dir_all(parent)?;
            // Add a small delay to ensure directory is fully created
            thread::sleep(Duration::from_millis(50));
        }
        
        let mut file = File::create(&file_path)?;
        file.write_all(content)?;
        file.sync_all()?;
        
        // Add a small delay to ensure file is fully written
        thread::sleep(Duration::from_millis(50));
        
        // Verify file was created
        if !file_path.exists() {
            return Err(io::Error::new(
                io::ErrorKind::Other,
                format!("Failed to verify file creation at {:?}", file_path)
            ));
        }
        
        // Verify file size
        let metadata = fs::metadata(&file_path)?;
        if metadata.len() != content.len() as u64 {
            return Err(io::Error::new(
                io::ErrorKind::Other,
                format!("File size mismatch. Expected {} bytes, got {} bytes", content.len(), metadata.len())
            ));
        }
        
        println!("Successfully created test file: {:?}", file_path);
        Ok(file_path)
    }

    #[test]
    fn collect_context_paths_parses_cli_arguments() {
        let args = vec![
            "BitBurn.exe".to_string(),
            "--context-wipe".to_string(),
            "C:/example/file1.txt".to_string(),
            "--other".to_string(),
            "D:/second.bin".to_string(),
        ];

        let collected = collect_context_paths(&args);
        assert_eq!(collected, vec![
            "C:/example/file1.txt".to_string(),
            "D:/second.bin".to_string(),
        ]);
    }

    #[test]
    fn collect_context_paths_splits_multi_value_argument() {
        let args = vec![
            "BitBurn.exe".to_string(),
            "--context-wipe".to_string(),
            "C:/one.txt|D:/two.txt;E:/three.txt\nF:/four.txt".to_string(),
        ];

        let collected = collect_context_paths(&args);
        assert_eq!(collected, vec![
            "C:/one.txt".to_string(),
            "D:/two.txt".to_string(),
            "E:/three.txt".to_string(),
            "F:/four.txt".to_string(),
        ]);
    }

    #[test]
    fn sanitize_context_paths_filters_invalid_entries() {
        let dir = create_test_dir().expect("should create temp dir");
        let valid_file = create_test_file(&dir, b"test").expect("should create file");
        let missing = dir.join("missing.bin");

        let payload = sanitize_context_paths(vec![
            valid_file.to_string_lossy().to_string(),
            "\\\\server\\share\\file.txt".to_string(),
            missing.to_string_lossy().to_string(),
        ]);

        assert_eq!(payload.paths.len(), 1);
        assert_eq!(payload.invalid.len(), 2);
        assert!(payload.paths[0].contains("test_file_"));
    }

    #[cfg(windows)]
    #[test]
    fn enable_disable_context_menu_respects_override_root() {
        let temp_root = format!(
            "Software\\Classes\\BitBurnTest_{}",
            get_unique_id()
        );
        std::env::set_var("BITBURN_CONTEXT_ROOT", &temp_root);

        let dummy_exe = PathBuf::from("C:/BitBurn/BitBurn.exe");

        enable_context_menu(&dummy_exe).expect("should write context menu keys");
        assert!(is_context_menu_enabled().unwrap());

        disable_context_menu().expect("should remove context menu keys");
        assert!(!is_context_menu_enabled().unwrap());

        // Cleanup env override
        std::env::remove_var("BITBURN_CONTEXT_ROOT");
    }

    #[test]
    fn validate_drive_path_rejects_non_root() {
        let temp_dir = std::env::temp_dir();
        let result = validate_drive_path_internal(temp_dir.as_path());
        assert!(matches!(result, Err(DriveValidationError::NotDriveRoot)));
    }

    #[test]
    fn cancelled_wipe_result_has_expected_message() {
        let result = cancelled_wipe_result();
        assert!(!result.success);
        assert_eq!(result.message, "Operation cancelled by user");
    }

    #[test]
    fn free_space_error_result_formats_message() {
        let result = free_space_error_result("sample error".to_string());
        assert!(!result.success);
        assert_eq!(result.message, "sample error");
    }

    fn cleanup_test_dir(dir: &Path) {
        println!("Cleaning up test directory: {:?}", dir);
        // Sleep briefly to ensure file handles are released
        thread::sleep(Duration::from_millis(50));
        if let Err(e) = fs::remove_dir_all(dir) {
            println!("Warning: Failed to clean up test directory: {:?} - {}", dir, e);
        }
    }

    #[test]
    fn test_nonexistent_file() {
        let dir = std::env::temp_dir();
        let file_path = dir.join("nonexistent_test_file");
        
        let result = secure_wipe_file(&file_path, 1, &WipeAlgorithm::NistClear, |_| {});
        assert!(matches!(result, Err(WipeError::PathNotFound)));
    }

    #[test]
    fn test_invalid_passes() -> io::Result<()> {
        let test_dir = create_test_dir()?;
        let test_data = [0xAA; 1024];
        let file_path = create_test_file(&test_dir, &test_data)?;
        
        let result = secure_wipe_file(&file_path, 0, &WipeAlgorithm::Random, |_| {});
        assert!(matches!(result, Err(WipeError::InvalidPasses)));
        
        cleanup_test_dir(&test_dir);
        Ok(())
    }

    #[test]
    fn test_nist_clear_wipe() -> io::Result<()> {
        let test_dir = create_test_dir()?;
        let test_data = [0xAA; 1024];
        let file_path = create_test_file(&test_dir, &test_data)?;
        
        // Verify file exists and has correct size
        let metadata = fs::metadata(&file_path)?;
        assert!(metadata.is_file(), "Created path should be a file");
        assert_eq!(metadata.len(), 1024, "File should be 1024 bytes");
        
        let mut progress_patterns_seen = Vec::new();
        let result = secure_wipe_file(&file_path, 1, &WipeAlgorithm::NistClear, |progress| {
            progress_patterns_seen.push(progress.current_pattern.clone());
        });
        
        // Verify the operation succeeded
        assert!(result.is_ok(), "Wipe operation should succeed: {:?}", result);
        
        // Verify progress messages contain "NIST Clear"
        for pattern in &progress_patterns_seen {
            assert!(pattern.contains("NIST 800-88 Clear"), 
                "Progress pattern should mention NIST Clear: {}", pattern);
        }
        
        // Verify file is deleted
        assert!(!file_path.exists(), "File should be deleted after wiping");
        
        cleanup_test_dir(&test_dir);
        Ok(())
    }

    #[test]
    fn test_nist_purge_wipe() -> io::Result<()> {
        let test_dir = create_test_dir()?;
        let test_data = [0xAA; 1024];
        let file_path = create_test_file(&test_dir, &test_data)?;
        
        // Verify file exists and has correct size
        let metadata = fs::metadata(&file_path)?;
        assert!(metadata.is_file(), "Created path should be a file");
        assert_eq!(metadata.len(), 1024, "File should be 1024 bytes");
        
        let mut progress_patterns_seen = Vec::new();
        let result = secure_wipe_file(&file_path, 3, &WipeAlgorithm::NistPurge, |progress| {
            progress_patterns_seen.push(progress.current_pattern.clone());
        });
        
        // Verify the operation succeeded
        assert!(result.is_ok(), "Wipe operation should succeed: {:?}", result);
        
        // Verify progress messages contain "NIST Purge"
        for pattern in &progress_patterns_seen {
            assert!(pattern.contains("NIST 800-88 Purge"), 
                "Progress pattern should mention NIST Purge: {}", pattern);
        }
        
        // Verify we saw all 3 passes
        assert!(progress_patterns_seen.iter().any(|p| p.contains("Pass 1/3")), 
            "Missing first pass");
        assert!(progress_patterns_seen.iter().any(|p| p.contains("Pass 2/3")), 
            "Missing second pass");
        assert!(progress_patterns_seen.iter().any(|p| p.contains("Pass 3/3")), 
            "Missing third pass");
        
        // Verify file is deleted
        assert!(!file_path.exists(), "File should be deleted after wiping");
        
        cleanup_test_dir(&test_dir);
        Ok(())
    }

    #[test]
    fn test_gutmann_wipe() -> io::Result<()> {
        let test_dir = create_test_dir()?;
        let test_data = [0xAA; 4096];  // Larger file for pattern testing
        let file_path = create_test_file(&test_dir, &test_data)?;
        
        let mut progress_patterns_seen = Vec::new();
        let result = secure_wipe_file(&file_path, 35, &WipeAlgorithm::Gutmann, |progress| {
            // Only store the base pattern without MB information
            let base_pattern = progress.current_pattern
                .split(" - ")
                .next()
                .unwrap_or(&progress.current_pattern)
                .to_string();
            if !progress_patterns_seen.contains(&base_pattern) {
                progress_patterns_seen.push(base_pattern);
            }
        });
        
        // Verify the operation succeeded
        assert!(result.is_ok(), "Wipe operation failed: {:?}", result);
        
        // Verify we saw all 35 passes
        let unique_passes = progress_patterns_seen.iter()
            .filter(|p| p.contains("Pass") || p.contains("Pattern"))
            .filter(|p| !p.contains("Finalizing"))
            .count();
        assert_eq!(unique_passes, 35, "Did not see all 35 passes");
            
        // Verify the sequence of passes
        let pass_sequence = progress_patterns_seen.iter()
            .map(|p| p.as_str())
            .collect::<Vec<_>>();
            
        // Verify first 4 passes are random
        for i in 0..4 {
            assert!(pass_sequence.iter().any(|&p| p.contains(&format!("Random data (Pass {}/35)", i + 1))),
                "Missing random pass {}", i + 1);
        }
        
        // Verify some key fixed patterns are present
        assert!(pass_sequence.iter().any(|&p| p.contains("Pattern 5/35: 0x55 0xAA")),
            "Missing alternating pattern 0x55 0xAA");
        assert!(pass_sequence.iter().any(|&p| p.contains("Pattern 7/35: 0x92 0x49 0x24")),
            "Missing pattern 0x92 0x49 0x24");
            
        // Verify last 4 passes are random
        for i in 32..=35 {
            assert!(pass_sequence.iter().any(|&p| p.contains(&format!("Random data (Pass {}/35)", i))),
                "Missing random pass {}", i);
        }
        
        // Verify file is deleted
        assert!(!file_path.exists(), "File should be deleted after wiping");
        
        cleanup_test_dir(&test_dir);
        Ok(())
    }

    #[test]
    fn test_random_wipe() -> io::Result<()> {
        let test_dir = create_test_dir()?;
        let test_data = [0xAA; 1024];
        let file_path = create_test_file(&test_dir, &test_data)?;
        
        // Test with 5 passes
        let passes = 5;
        let mut progress_patterns_seen = Vec::new();
        let result = secure_wipe_file(&file_path, passes, &WipeAlgorithm::Random, |progress| {
            // Only store the base pattern without MB information
            let base_pattern = progress.current_pattern
                .split(" - ")
                .next()
                .unwrap_or(&progress.current_pattern)
                .to_string();
            if !progress_patterns_seen.contains(&base_pattern) {
                progress_patterns_seen.push(base_pattern);
            }
        });
        
        // Verify the operation succeeded
        assert!(result.is_ok(), "Wipe operation should succeed: {:?}", result);
        
        // Verify we saw all passes
        let unique_passes = progress_patterns_seen.iter()
            .filter(|p| p.contains("Pass"))
            .filter(|p| !p.contains("Finalizing"))
            .count();
        assert_eq!(unique_passes, passes as usize, "Did not see all passes");
        
        // Verify pass numbering
        for i in 1..=passes {
            let pass_pattern = format!("Writing random data (Pass {}/{})", i, passes);
            assert!(progress_patterns_seen.iter().any(|p| p == &pass_pattern),
                "Missing pass {}", i);
        }
        
        // Verify file is deleted
        assert!(!file_path.exists(), "File should be deleted after wiping");
        
        cleanup_test_dir(&test_dir);
        Ok(())
    }

    #[cfg(not(windows))]
    #[test]
    fn platform_info_reports_non_windows() {
        let info = tauri::async_runtime::block_on(platform_info()).expect("platform_info should succeed");
        assert!(!info.is_windows);
        assert_ne!(info.os, "windows");
    }

    #[cfg(windows)]
    #[test]
    fn platform_info_reports_windows() {
        let info = tauri::async_runtime::block_on(platform_info()).expect("platform_info should succeed");
        assert!(info.is_windows);
        assert_eq!(info.os, "windows");
    }
}
