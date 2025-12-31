// Prevents additional console window on Windows in release
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

use rand::RngCore;
use serde::{Deserialize, Serialize};
use serde_json::json;
use std::fs::{self, OpenOptions};
use std::io::{self, Seek, SeekFrom, Write};
use std::path::Path;
use tauri::{
    menu::{Menu, MenuItem},
    tray::{MouseButton, MouseButtonState, TrayIconBuilder, TrayIconEvent},
    Emitter, Listener, Manager, Runtime, WindowEvent,
};
use walkdir::WalkDir;
use std::fmt;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use sysinfo::{DiskExt, System, SystemExt};
mod platform;

use platform::context_menu::{
    get_context_menu_status,
    handle_context_invocation,
    process_cli_side_effects,
    register_context_menu,
    unregister_context_menu,
};

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

#[derive(Debug, Serialize, Deserialize)]
pub enum WipeAlgorithm {
    NistClear,      // NIST 800-88 Clear: 1 pass zeros (replaces Basic)
    NistPurge,      // NIST 800-88 Purge: 3 pass overwrite (replaces DOD)
    Gutmann,        // 35 pass: Gutmann pattern (kept for legacy/specific needs)
    Random,         // N passes of random data (replaces DOD_E and custom needs)
}

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

#[derive(Serialize)]
pub struct WipeResult {
    success: bool,
    message: String,
}

#[derive(Serialize)]
pub struct ContextMenuStatus {
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
#[tauri::command]
async fn execute_free_space_wipe<R: Runtime>(
    window: tauri::Window<R>,
    path: String,
    algorithm: WipeAlgorithm,
    passes: u32
) -> Result<WipeResult, String> {
    log_event(
        "wipe_free_space_start",
        json!({"path": path, "algorithm": format!("{:?}", algorithm), "passes": passes}),
    );
    
    let path = Path::new(&path);
    let cancelled = Arc::new(AtomicBool::new(false));
    let cancelled_clone = cancelled.clone();
    
    // Set up cancellation handler
    let _unregister = window.once("cancel_operation", move |_| {
        cancelled_clone.store(true, Ordering::SeqCst);
    });
    
    // Validate again just to be safe
    if let Err(e) = validate_drive_path_internal(path) {
        return Ok(WipeResult {
            success: false,
            message: e.to_string(),
        });
    }

    // Initialize system info
    let mut sys = System::new_all();
    sys.refresh_disks_list();
    
    // Find the disk that contains our path
    let disk_info = sys.disks().iter()
        .find(|disk| path.starts_with(disk.mount_point()))
        .ok_or_else(|| "Could not find disk information".to_string())?;
    
    let available_space = disk_info.available_space();
    println!("Available space on drive: {} bytes", available_space);

    let window_clone = window.clone();
    let cancelled_clone = cancelled.clone();
    let progress_callback = move |progress| {
        if !cancelled_clone.load(Ordering::SeqCst) {
            let _ = window_clone.emit_to("main", "wipe_progress", progress);
        }
    };

    let mut progress = WipeProgress::new(
        passes,
        0,
        match algorithm {
            WipeAlgorithm::NistClear => "NIST 800-88 Clear",
            WipeAlgorithm::NistPurge => "NIST 800-88 Purge",
            WipeAlgorithm::Gutmann => "Gutmann",
            WipeAlgorithm::Random => "Random",
        }
    );
    
    // Set the estimated total bytes to the available space
    progress.estimated_total_bytes = Some(available_space);

    // Create and fill temporary file
    println!("Creating temporary file");
    progress.update(0, "Filling drive space");
    progress_callback(progress.clone());

    let temp_file_path = path.join(".temp_wipe_file");
    
    // Check for existing temp file
    if temp_file_path.exists() {
        println!("Existing temporary file found, attempting to remove");
        progress.update(0, "Cleaning up previous temporary file");
        progress_callback(progress.clone());
        if let Err(e) = fs::remove_file(&temp_file_path) {
            return Ok(WipeResult {
                success: false,
                message: format!("Failed to remove existing temporary file: {}", e),
            });
        }
    }

    let mut file = match OpenOptions::new()
        .write(true)
        .create(true)
        .open(&temp_file_path) {
            Ok(f) => f,
            Err(e) => {
                return Ok(WipeResult {
                    success: false,
                    message: format!("Failed to create temporary file: {}", e),
                });
            }
    };

    // Write data in chunks until disk is full
    let chunk_size = 1024 * 1024; // 1MB chunks
    let mut buffer = vec![0u8; chunk_size];
    let mut rng = rand::thread_rng();
    let mut total_written = 0u64;
    let mut last_refresh = std::time::Instant::now();
    let mut last_space_used = 0u64;

    loop {
        // Check for cancellation
        if cancelled.load(Ordering::SeqCst) {
            let _ = file.sync_all();
            let _ = fs::remove_file(&temp_file_path);
            return Ok(WipeResult {
                success: false,
                message: "Operation cancelled by user".to_string(),
            });
        }

        // Refresh disk info every 100ms to avoid excessive system calls
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
                
                // Update progress after every chunk write
                progress.update(last_space_used, &format!("Filling drive space ({} MB written)", total_written / 1024 / 1024));
                progress_callback(progress.clone());
                
                if total_written % (10 * chunk_size as u64) == 0 {
                    if let Err(_) = file.sync_all() {
                        break;
                    }
                }
            },
            Err(e) => {
                if e.kind() == io::ErrorKind::StorageFull || 
                   e.kind() == io::ErrorKind::OutOfMemory ||
                   e.kind() == io::ErrorKind::WriteZero {
                    // One final refresh of disk info
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
                return Ok(WipeResult {
                    success: false,
                    message: format!("Failed to write to temporary file: {}", e),
                });
            }
        }
    }

    // Now wipe the temporary file
    progress.total_bytes = total_written;
    let cancelled_clone = cancelled.clone();
    match secure_wipe_file(&temp_file_path, passes, &algorithm, move |p| {
        // Check for cancellation during wiping
        if !cancelled_clone.load(Ordering::SeqCst) {
            progress_callback(p);
        }
    }) {
        Ok(_) => {
            if cancelled.load(Ordering::SeqCst) {
                log_event("wipe_free_space_cancelled", json!({"path": path.to_string_lossy()}));
                Ok(WipeResult {
                    success: false,
                    message: "Operation cancelled by user".to_string(),
                })
            } else {
                log_event("wipe_free_space_complete", json!({"path": path.to_string_lossy(), "status": "success"}));
                Ok(WipeResult {
                    success: true,
                    message: format!("Successfully wiped free space"),
                })
            }
        },
        Err(e) => {
            let _ = fs::remove_file(&temp_file_path);
            log_event(
                "wipe_free_space_error",
                json!({"path": path.to_string_lossy(), "message": format!("{}", e)}),
            );
            Ok(WipeResult {
                success: false,
                message: format!("Failed to wipe free space: {}", e),
            })
        },
    }
}

/// Securely wipe files or folders using the selected algorithm.
#[tauri::command]
async fn wipe_files<R: Runtime>(
    window: tauri::Window<R>,
    paths: Vec<String>,
    passes: u32,
    algorithm: WipeAlgorithm
) -> Result<WipeResult, String> {
    log_event(
        "wipe_files_start",
        json!({"count": paths.len(), "algorithm": format!("{:?}", algorithm), "passes": passes}),
    );
    let mut total_files = 0;
    let mut failed_files = Vec::new();
    let cancelled = Arc::new(AtomicBool::new(false));
    let cancelled_clone = cancelled.clone();

    // Set up cancellation handler
    let _unregister = window.once("cancel_operation", move |_| {
        cancelled_clone.store(true, Ordering::SeqCst);
    });

    for path_str in paths {
        if cancelled.load(Ordering::SeqCst) {
            return Ok(WipeResult {
                success: false,
                message: "Operation cancelled by user".to_string(),
            });
        }

        let path = Path::new(&path_str);
        
        if !path.exists() {
            failed_files.push(format!("Path not found: {}", path_str));
            continue;
        }

        if path.is_file() {
            let window_clone = window.clone();
            let cancelled_clone = cancelled.clone();
            match secure_wipe_file(
                path,
                passes,
                &algorithm,
                move |progress| {
                    if !cancelled_clone.load(Ordering::SeqCst) {
                        let _ = window_clone.emit_to("main", "wipe_progress", progress);
                    }
                }
            ) {
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

                let window_clone = window.clone();
                let cancelled_clone = cancelled.clone();
                match secure_wipe_file(
                    entry.path(),
                    passes,
                    &algorithm,
                    move |progress| {
                        if !cancelled_clone.load(Ordering::SeqCst) {
                            let _ = window_clone.emit_to("main", "wipe_progress", progress);
                        }
                    }
                ) {
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
        let result = WipeResult {
            success: false,
            message: "Operation cancelled by user".to_string(),
        };
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
}

fn main() {
    let args: Vec<String> = std::env::args().collect();
    if let Some(code) = process_cli_side_effects(&args, log_event) {
        std::process::exit(code);
    }

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
            platform_info
        ])
        .setup(|app| {
            let initial_args: Vec<String> = std::env::args().collect();
            handle_context_invocation(&app.app_handle(), &initial_args);

            // Set up window close handler
            if let Some(window) = app.get_webview_window("main") {
                let window_clone = window.clone();
                window.on_window_event(move |event| {
                    if let WindowEvent::CloseRequested { api, .. } = event {
                        window_clone.hide().unwrap();
                        api.prevent_close();
                    }
                });
            }

            // Position and show the main window on launch
            if let Some(window) = app.get_webview_window("main") {
                let window_clone = window.clone();
                tauri::async_runtime::spawn(async move {
                    let _ = window_clone.center();
                    if let Some(monitor) = window_clone.current_monitor().ok().flatten() {
                        let monitor_size = monitor.size();
                        let height_percentage = 0.80;
                        let window_height = (monitor_size.height as f64 * height_percentage) as u32;
                        
                        // Set the window size to use the percentage of screen height
                        let _ = window_clone.set_size(tauri::Size::Physical(tauri::PhysicalSize {
                            width: window_clone.outer_size().unwrap().width,
                            height: window_height,
                        }));
                        
                        // Center the window after resizing
                        let _ = window_clone.center();
                    }
                    let _ = window_clone.show();
                    let _ = window_clone.set_focus();
                });
            }

            // Create menu items
            let quit_i = MenuItem::with_id(app, "quit", "Quit", true, None::<&str>)?;

            // Create the menu
            let menu = Menu::with_items(app, &[&quit_i])?;

            // Build the tray
            let _tray = TrayIconBuilder::new()
                .icon(app.default_window_icon().unwrap().clone())
                .menu(&menu)
                .show_menu_on_left_click(false)
                .on_menu_event(|app, event| match event.id.as_ref() {
                    "quit" => {
                        app.exit(0);
                    }
                    _ => {}
                })
                .on_tray_icon_event(|tray, event| match event {
                    TrayIconEvent::Click {
                        button: MouseButton::Left,
                        button_state: MouseButtonState::Up,
                        ..
                    } => {
                        let app = tray.app_handle();
                        if let Some(window) = app.get_webview_window("main") {
                            if window.is_visible().unwrap_or(false) {
                                let _ = window.hide();
                            } else {
                                let _ = window.show();
                            }
                        }
                    }
                    _ => {}
                })
                .build(app)?;

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
}
